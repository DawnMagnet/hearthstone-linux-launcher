use super::{partition_hash, verify_md5};
use crate::download;
use anyhow::{Context, Result};
use std::{
    io::{ErrorKind, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tracing::{debug, trace, warn};
use url::Url;

#[derive(Clone)]
pub struct RemoteCdn {
    http: reqwest::Client,
    server: Url,
    data_base: String,
    config_base: String,
    cache_dir: Option<PathBuf>,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<dyn Fn(u64) + Send + Sync>>,
    last_transfer: Arc<Mutex<Option<TransferStats>>>,
}

#[derive(Clone, Debug)]
struct TransferStats {
    bytes: usize,
    elapsed: Duration,
    from_cache: bool,
}

impl RemoteCdn {
    pub fn from_forced_url(http: reqwest::Client, forced_url: &str) -> Result<Self> {
        let url = Url::parse(forced_url)?;
        let server = Url::parse(&format!(
            "{}://{}",
            url.scheme(),
            url.host_str().context("CDN URL has no host")?
        ))?;
        let data_base = url.path().trim_end_matches('/').to_string();
        debug!(cdn = forced_url, "created remote CDN");
        Ok(Self {
            http,
            server,
            data_base,
            config_base: "/tpr/configs/data".to_string(),
            cache_dir: None,
            cancel: None,
            progress: None,
            last_transfer: Arc::new(Mutex::new(None)),
        })
    }

    pub fn with_cache_dir(mut self, cache_dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(cache_dir.into());
        self
    }

    pub fn with_cancel_token(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = Some(cancel);
        self
    }

    pub fn with_progress_callback(mut self, progress: Arc<dyn Fn(u64) + Send + Sync>) -> Self {
        self.progress = Some(progress);
        self
    }

    pub fn last_transfer_label(&self) -> Option<String> {
        let stats = self.last_transfer.lock().ok()?.clone()?;
        if stats.from_cache {
            return Some("cache".to_string());
        }

        let seconds = stats.elapsed.as_secs_f64();
        if seconds <= f64::EPSILON {
            return None;
        }

        Some(format!("{}/s", format_bytes(stats.bytes as f64 / seconds)))
    }

    pub async fn fetch_config(&self, key: &str, verify: bool) -> Result<Vec<u8>> {
        let path = format!("/config/{}", partition_hash(key)?);
        self.fetch_hashed("config file", "config", key, &self.data_base, &path, verify)
            .await
    }

    pub async fn fetch_config_data(&self, key: &str, verify: bool) -> Result<Vec<u8>> {
        let path = format!("/{}", partition_hash(key)?);
        self.fetch_hashed(
            "config item",
            "config-data",
            key,
            &self.config_base,
            &path,
            verify,
        )
        .await
    }

    pub async fn fetch_data(&self, key: &str, verify: bool) -> Result<Vec<u8>> {
        let path = format!("/data/{}", partition_hash(key)?);
        self.fetch_hashed("data file", "data", key, &self.data_base, &path, verify)
            .await
    }

    pub async fn fetch_data_range(&self, key: &str, offset: usize, size: usize) -> Result<Vec<u8>> {
        if let Some(data) = self.read_cached_range("data", key, offset, size).await? {
            trace!(
                namespace = "data",
                key = %key,
                offset = offset,
                bytes = data.len(),
                "cache range hit"
            );
            return Ok(data);
        }

        let path = format!("/data/{}", partition_hash(key)?);
        let data = match self
            .fetch_joined_range(&self.data_base, &path, offset, size)
            .await
        {
            Ok(data) => data,
            Err(error) => {
                warn!(
                    namespace = "data",
                    key = %key,
                    offset = offset,
                    bytes = size,
                    error = %format!("{error:#}"),
                    "CDN range fetch failed; falling back to full file"
                );
                let data = self.fetch_data(key, false).await?;
                let end = offset
                    .checked_add(size)
                    .context("requested CDN data range overflows")?;
                anyhow::ensure!(end <= data.len(), "CDN data range exceeds file size");
                data[offset..end].to_vec()
            }
        };
        Ok(data)
    }

    async fn fetch_joined_range(
        &self,
        base: &str,
        path: &str,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>> {
        let mut url = self.server.clone();
        url.set_path(&format!(
            "{}/{}",
            base.trim_matches('/'),
            path.trim_start_matches('/')
        ));

        let offset_u64 = u64::try_from(offset).context("range offset is too large")?;
        let size_u64 = u64::try_from(size).context("range size is too large")?;
        let start = Instant::now();
        let data = download::download_range_to_vec(
            &self.http,
            url.clone(),
            offset_u64,
            size_u64,
            self.cancel.clone(),
            self.progress.clone(),
        )
        .await?;
        debug!(url = %url, offset = offset, bytes = data.len(), elapsed_ms = start.elapsed().as_millis(), "fetched CDN URL range");
        self.record_transfer(TransferStats {
            bytes: data.len(),
            elapsed: start.elapsed(),
            from_cache: false,
        });
        Ok(data)
    }

    pub async fn fetch_data_optional(&self, key: &str, verify: bool) -> Result<Option<Vec<u8>>> {
        if let Some(data) = self.read_cached("data", key).await? {
            if !verify || data_md5_matches(&data, key) {
                trace!(namespace = "data", key = %key, bytes = data.len(), "cache hit");
                return Ok(Some(data));
            }
            warn!(namespace = "data", key = %key, "cached data failed md5 verification");
            self.remove_cached("data", key).await;
        }

        let path = format!("/data/{}", partition_hash(key)?);
        match self.fetch_joined_optional(&self.data_base, &path).await? {
            Some(data) => {
                if verify && !data_md5_matches(&data, key) {
                    warn!(namespace = "data", key = %key, bytes = data.len(), "downloaded data failed md5 verification");
                    return Ok(None);
                }
                self.write_cached("data", key, &data).await;
                Ok(Some(data))
            }
            None => Ok(None),
        }
    }

    pub async fn fetch_data_optional_unverified(&self, key: &str) -> Result<Option<Vec<u8>>> {
        if let Some(data) = self.read_cached("data", key).await? {
            trace!(namespace = "data", key = %key, bytes = data.len(), "unverified cache hit");
            return Ok(Some(data));
        }

        let path = format!("/data/{}", partition_hash(key)?);
        self.fetch_joined_optional(&self.data_base, &path).await
    }

    pub async fn read_data_cache_unverified(&self, key: &str) -> Result<Option<Vec<u8>>> {
        if let Some(data) = self.read_cached("data", key).await? {
            trace!(namespace = "data", key = %key, bytes = data.len(), "unverified cache hit");
            return Ok(Some(data));
        }

        Ok(None)
    }

    pub async fn cache_data(&self, key: &str, data: &[u8]) {
        self.write_cached("data", key, data).await;
    }

    pub async fn remove_data_cache(&self, key: &str) {
        self.remove_cached("data", key).await;
    }

    pub async fn fetch_data_index(&self, key: &str, verify: bool) -> Result<Vec<u8>> {
        if let Some(data) = self.read_cached("data-index", key).await? {
            if verify_index_footer(&data, key, verify).is_ok() {
                trace!(namespace = "data-index", key = %key, bytes = data.len(), "cache hit");
                return Ok(data);
            }
            warn!(namespace = "data-index", key = %key, "cached archive index failed verification");
            self.remove_cached("data-index", key).await;
        }

        let path = format!("/data/{}.index", partition_hash(key)?);
        let data = self.fetch_joined(&self.data_base, &path).await?;
        verify_index_footer(&data, key, verify)?;
        self.write_cached("data-index", key, &data).await;
        Ok(data)
    }

    async fn fetch_hashed(
        &self,
        name: &str,
        namespace: &str,
        key: &str,
        base: &str,
        path: &str,
        verify: bool,
    ) -> Result<Vec<u8>> {
        if let Some(data) = self.read_cached(namespace, key).await? {
            if verify_md5(name, &data, key, verify).is_ok() {
                trace!(namespace = %namespace, key = %key, bytes = data.len(), "cache hit");
                return Ok(data);
            }
            warn!(namespace = %namespace, key = %key, name = %name, "cached item failed md5 verification");
            self.remove_cached(namespace, key).await;
        }

        let data = self.fetch_joined(base, path).await?;
        verify_md5(name, &data, key, verify)?;
        self.write_cached(namespace, key, &data).await;
        Ok(data)
    }

    async fn fetch_joined(&self, base: &str, path: &str) -> Result<Vec<u8>> {
        self.fetch_joined_optional(base, path)
            .await?
            .with_context(|| format!("CDN item not found: {base}{path}"))
    }

    async fn fetch_joined_optional(&self, base: &str, path: &str) -> Result<Option<Vec<u8>>> {
        let mut url = self.server.clone();
        url.set_path(&format!(
            "{}/{}",
            base.trim_matches('/'),
            path.trim_start_matches('/')
        ));

        let mut last_error = None;
        for attempt in 1..=5 {
            self.check_cancelled()?;
            debug!(url = %url, attempt = attempt, "fetching CDN URL");
            match self.fetch_url_optional(url.clone()).await {
                Ok(result) => {
                    if result.is_none() {
                        debug!(url = %url, attempt = attempt, "CDN URL returned 404");
                    }
                    return Ok(result);
                }
                Err(error) => {
                    warn!(url = %url, attempt = attempt, error = %format!("{error:#}"), "CDN fetch attempt failed");
                    last_error = Some(error);
                    self.check_cancelled()?;
                    if attempt < 5 {
                        tokio::time::sleep(Duration::from_secs(attempt)).await;
                    }
                }
            }
        }

        Err(last_error.expect("at least one fetch attempt failed"))
    }

    async fn fetch_url_optional(&self, url: Url) -> Result<Option<Vec<u8>>> {
        self.check_cancelled()?;
        if let Ok(response) = self.http.head(url.clone()).send().await {
            self.check_cancelled()?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            response.error_for_status_ref()?;
        }
        let start = Instant::now();
        let data = download::download_to_vec(
            &self.http,
            url.clone(),
            self.cancel.clone(),
            self.progress.clone(),
        )
        .await?;
        self.check_cancelled()?;
        debug!(url = %url, bytes = data.len(), elapsed_ms = start.elapsed().as_millis(), "fetched CDN URL");
        self.record_transfer(TransferStats {
            bytes: data.len(),
            elapsed: start.elapsed(),
            from_cache: false,
        });
        Ok(Some(data))
    }

    async fn read_cached(&self, namespace: &str, key: &str) -> Result<Option<Vec<u8>>> {
        let Some(path) = self.cache_path(namespace, key)? else {
            return Ok(None);
        };

        match tokio::fs::read(&path).await {
            Ok(data) => {
                trace!(path = %path.display(), bytes = data.len(), "read cache file");
                self.record_transfer(TransferStats {
                    bytes: data.len(),
                    elapsed: Duration::ZERO,
                    from_cache: true,
                });
                Ok(Some(data))
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(_) => {
                warn!(path = %path.display(), "failed to read cache file; removing it");
                let _ = tokio::fs::remove_file(&path).await;
                Ok(None)
            }
        }
    }

    async fn read_cached_range(
        &self,
        namespace: &str,
        key: &str,
        offset: usize,
        size: usize,
    ) -> Result<Option<Vec<u8>>> {
        let Some(path) = self.cache_path(namespace, key)? else {
            return Ok(None);
        };
        let end = offset
            .checked_add(size)
            .context("requested cache range overflows")?;
        let offset_u64 = u64::try_from(offset).context("cache range offset is too large")?;
        let end_u64 = u64::try_from(end).context("cache range end is too large")?;

        let mut file = match tokio::fs::File::open(&path).await {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(_) => {
                warn!(path = %path.display(), "failed to open cache file; removing it");
                let _ = tokio::fs::remove_file(&path).await;
                return Ok(None);
            }
        };

        match file.metadata().await {
            Ok(metadata) if end_u64 <= metadata.len() => {}
            Ok(metadata) => {
                warn!(
                    path = %path.display(),
                    offset = offset,
                    bytes = size,
                    file_bytes = metadata.len(),
                    "cache file is too short for requested range; removing it"
                );
                let _ = tokio::fs::remove_file(&path).await;
                return Ok(None);
            }
            Err(_) => {
                warn!(path = %path.display(), "failed to stat cache file; removing it");
                let _ = tokio::fs::remove_file(&path).await;
                return Ok(None);
            }
        }

        let mut data = vec![0; size];
        if file.seek(SeekFrom::Start(offset_u64)).await.is_err()
            || file.read_exact(&mut data).await.is_err()
        {
            warn!(
                path = %path.display(),
                offset = offset,
                bytes = size,
                "failed to read cache range; removing it"
            );
            let _ = tokio::fs::remove_file(&path).await;
            return Ok(None);
        }

        trace!(
            path = %path.display(),
            offset = offset,
            bytes = data.len(),
            "read cache file range"
        );
        self.record_transfer(TransferStats {
            bytes: data.len(),
            elapsed: Duration::ZERO,
            from_cache: true,
        });
        Ok(Some(data))
    }

    async fn write_cached(&self, namespace: &str, key: &str, data: &[u8]) {
        let Ok(Some(path)) = self.cache_path(namespace, key) else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        if tokio::fs::create_dir_all(parent).await.is_err() {
            return;
        }

        let temp = temp_cache_path(&path);
        if tokio::fs::write(&temp, data).await.is_ok() {
            let _ = tokio::fs::rename(temp, path).await;
            trace!(namespace = %namespace, key = %key, bytes = data.len(), "wrote cache item");
        }
    }

    async fn remove_cached(&self, namespace: &str, key: &str) {
        if let Ok(Some(path)) = self.cache_path(namespace, key) {
            trace!(namespace = %namespace, key = %key, path = %path.display(), "removing cache item");
            let _ = tokio::fs::remove_file(path).await;
        }
    }

    fn cache_path(&self, namespace: &str, key: &str) -> Result<Option<PathBuf>> {
        let Some(cache_dir) = &self.cache_dir else {
            return Ok(None);
        };
        Ok(Some(cache_dir.join(namespace).join(partition_hash(key)?)))
    }

    fn record_transfer(&self, stats: TransferStats) {
        if let Ok(mut last_transfer) = self.last_transfer.lock() {
            *last_transfer = Some(stats);
        }
    }

    fn check_cancelled(&self) -> Result<()> {
        if self
            .cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
        {
            warn!("CDN fetch cancelled");
            anyhow::bail!("installation cancelled");
        }
        Ok(())
    }
}

fn data_md5_matches(data: &[u8], expected: &str) -> bool {
    format!("{:x}", md5::compute(data)) == expected
}

fn verify_index_footer(data: &[u8], key: &str, verify: bool) -> Result<()> {
    anyhow::ensure!(data.len() >= 28, "archive index is too short");
    verify_md5(
        "archive index footer",
        &data[data.len() - 28..],
        key,
        verify,
    )
}

fn temp_cache_path(path: &Path) -> PathBuf {
    let mut temp = path.to_path_buf();
    let extension = format!("tmp-{}", std::process::id());
    temp.set_extension(extension);
    temp
}

fn format_bytes(bytes: f64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes;
    let mut unit = UNITS[0];
    for candidate in UNITS.iter().skip(1) {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = candidate;
    }

    if unit == "B" {
        format!("{value:.0} {unit}")
    } else {
        format!("{value:.1} {unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteCdn;

    fn cached_test_cdn(cache_dir: &std::path::Path) -> RemoteCdn {
        RemoteCdn::from_forced_url(reqwest::Client::new(), "https://example.com/tpr/hs")
            .unwrap()
            .with_cache_dir(cache_dir)
    }

    #[tokio::test]
    async fn read_cached_range_returns_requested_slice() {
        let temp = tempfile::tempdir().unwrap();
        let cdn = cached_test_cdn(temp.path());
        let key = "0123456789abcdef";
        cdn.cache_data(key, b"0123456789").await;

        let data = cdn
            .read_cached_range("data", key, 2, 5)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(data, b"23456");
    }

    #[tokio::test]
    async fn read_cached_range_removes_short_cache_file() {
        let temp = tempfile::tempdir().unwrap();
        let cdn = cached_test_cdn(temp.path());
        let key = "0123456789abcdef";
        cdn.cache_data(key, b"short").await;
        let path = cdn.cache_path("data", key).unwrap().unwrap();

        let data = cdn.read_cached_range("data", key, 2, 8).await.unwrap();

        assert!(data.is_none());
        assert!(!path.exists());
    }
}
