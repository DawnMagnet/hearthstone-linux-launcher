pub mod archive;
pub mod blizini;
pub mod blte;
pub mod cdn;
pub mod configfile;
pub mod encoding;
pub mod installfile;
pub mod psv;

use crate::{Locale, Region};
use anyhow::{Context, Result};
use cdn::RemoteCdn;
use configfile::{BuildConfig, CdnConfig};
use encoding::EncodingFile;
use installfile::{InstallEntry, InstallFile};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinSet};
use tracing::{debug, info, trace, warn};

const INSTALL_FILE_CONCURRENCY: usize = 8;

#[derive(Clone, Debug)]
pub struct VersionInfo {
    pub region: String,
    pub build_config: String,
    pub cdn_config: String,
    pub build_id: String,
    pub version_name: String,
    pub product_config: Option<String>,
}

#[derive(Clone, Debug)]
pub struct InstallOptions {
    pub region: Region,
    pub locale: Locale,
    pub verify: bool,
}

#[derive(Clone, Debug)]
pub struct ProgressUpdate {
    pub message: String,
    pub fraction: Option<f64>,
}

impl ProgressUpdate {
    pub fn new(message: impl Into<String>, fraction: impl Into<Option<f64>>) -> Self {
        Self {
            message: message.into(),
            fraction: fraction.into().map(|value| value.clamp(0.0, 1.0)),
        }
    }
}

pub struct NgdpClient {
    http: reqwest::Client,
    cache_dir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
struct InstallWorkItem {
    entry: InstallEntry,
    encoding_key: String,
    has_archive: bool,
}

impl Default for NgdpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl NgdpClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(30))
                .build()
                .expect("reqwest client"),
            cache_dir: None,
        }
    }

    pub fn with_cache_dir(mut self, cache_dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(cache_dir.into());
        self
    }

    pub async fn latest_version(&self, region: Region) -> Result<VersionInfo> {
        let mut errors = Vec::new();

        for url in version_urls() {
            match self.fetch_latest_version(region, &url).await {
                Ok(version) => {
                    info!(
                        region = %region,
                        source = %url,
                        version = %version.version_name,
                        build_id = %version.build_id,
                        "found latest version"
                    );
                    return Ok(version);
                }
                Err(error) => {
                    warn!(region = %region, source = %url, error = %format!("{error:#}"), "version metadata fetch failed");
                    errors.push(format!("{url}: {error:#}"));
                }
            }
        }

        anyhow::bail!(
            "could not fetch Hearthstone version metadata for region {region}; tried: {}",
            errors.join("; ")
        )
    }

    async fn fetch_latest_version(&self, region: Region, url: &str) -> Result<VersionInfo> {
        let text = self
            .http
            .get(url)
            .send()
            .await
            .with_context(|| format!("request failed for {url}"))?
            .error_for_status()
            .with_context(|| format!("server rejected {url}"))?
            .text()
            .await
            .with_context(|| format!("failed to read response from {url}"))?;
        debug!(region = %region, source = %url, bytes = text.len(), "read version metadata");
        let psv = psv::PsvFile::parse(&text)?;

        psv.rows
            .into_iter()
            .find(|row| row.get("Region").map(String::as_str) == Some(region.as_str()))
            .map(|row| VersionInfo {
                region: row.get("Region").cloned().unwrap_or_default(),
                build_config: row.get("BuildConfig").cloned().unwrap_or_default(),
                cdn_config: row.get("CDNConfig").cloned().unwrap_or_default(),
                build_id: row.get("BuildId").cloned().unwrap_or_default(),
                version_name: row.get("VersionsName").cloned().unwrap_or_default(),
                product_config: row.get("ProductConfig").cloned(),
            })
            .with_context(|| format!("no version entry found for region {region} in {url}"))
    }

    pub async fn install_latest(
        &self,
        options: &InstallOptions,
        out_dir: &Path,
        mut progress: impl FnMut(ProgressUpdate) + Send,
    ) -> Result<VersionInfo> {
        self.install_latest_with_cancel(options, out_dir, &mut progress, None)
            .await
    }

    pub async fn install_latest_cancellable(
        &self,
        options: &InstallOptions,
        out_dir: &Path,
        mut progress: impl FnMut(ProgressUpdate) + Send,
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<VersionInfo> {
        self.install_latest_with_cancel(options, out_dir, &mut progress, cancel)
            .await
    }

    async fn install_latest_with_cancel(
        &self,
        options: &InstallOptions,
        out_dir: &Path,
        progress: &mut (impl FnMut(ProgressUpdate) + Send),
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<VersionInfo> {
        check_cancelled(cancel.as_ref())?;
        info!(
            region = %options.region,
            locale = %options.locale,
            out_dir = %out_dir.display(),
            verify = options.verify,
            "starting NGDP install"
        );
        progress(ProgressUpdate::new(
            "Checking latest Hearthstone version",
            0.02,
        ));
        let version = self.latest_version(options.region).await?;
        check_cancelled(cancel.as_ref())?;

        let mut cdn = RemoteCdn::from_forced_url(self.http.clone(), options.region.default_cdn())?;
        if let Some(cache_dir) = &self.cache_dir {
            cdn = cdn.with_cache_dir(cache_dir);
        }
        if let Some(cancel) = &cancel {
            cdn = cdn.with_cancel_token(cancel.clone());
        }
        debug!(cdn = %options.region.default_cdn(), "configured CDN");

        progress(ProgressUpdate::new(
            format!("Fetching build config {}", version.build_config),
            0.06,
        ));
        let build_config = BuildConfig::parse(
            &cdn.fetch_config(&version.build_config, options.verify)
                .await?,
        )?;
        debug!(
            build = %version.build_config,
            build_name = %build_config.build_name,
            root = %build_config.root,
            install_content = %build_config.install.content_key,
            install_encoding = %build_config.install.encoding_key,
            encoding_content = %build_config.encoding.content_key,
            encoding_key = %build_config.encoding.encoding_key,
            "parsed build config"
        );
        check_cancelled(cancel.as_ref())?;

        progress(ProgressUpdate::new(
            format!("Fetching CDN config {}", version.cdn_config),
            0.10,
        ));
        let cdn_config = CdnConfig::parse(
            &cdn.fetch_config(&version.cdn_config, options.verify)
                .await?,
        )?;
        debug!(
            cdn_config = %version.cdn_config,
            archive_group = %cdn_config.archive_group,
            archive_count = cdn_config.archives.len(),
            "parsed CDN config"
        );
        check_cancelled(cancel.as_ref())?;

        progress(ProgressUpdate::new("Fetching encoding table", 0.14));
        let encoding = self
            .fetch_encoding(&cdn, &build_config, options.verify)
            .await?;
        check_cancelled(cancel.as_ref())?;

        progress(ProgressUpdate::new("Fetching install manifest", 0.18));
        let install = self
            .fetch_install_manifest(&cdn, &build_config, &encoding, options.verify)
            .await?;
        check_cancelled(cancel.as_ref())?;

        let tags = ["OSX", options.locale.as_str(), "Production"];
        let entries = install.filter_entries(&tags)?;
        info!(
            tags = ?tags,
            entries = entries.len(),
            "filtered install manifest"
        );
        progress(ProgressUpdate::new(
            format!("Installing {} files", entries.len()),
            0.22,
        ));

        let archive_map =
            archive::ArchiveMap::load(&cdn, &cdn_config, options.verify, |message, fraction| {
                progress(ProgressUpdate::new(
                    message,
                    fraction.map(|value| 0.22 + value * 0.13),
                ))
            })
            .await?;
        check_cancelled(cancel.as_ref())?;

        let mut work = Vec::with_capacity(entries.len());
        for entry in entries {
            let encoding_key = encoding
                .find_by_content_key(&entry.content_key)
                .with_context(|| format!("encoding key not found for {}", entry.path))?;
            trace!(
                path = %entry.path,
                content_key = %entry.content_key,
                encoding_key,
                in_archive = archive_map.contains(encoding_key),
                "resolved install entry encoding key"
            );
            work.push(InstallWorkItem {
                has_archive: archive_map.contains(encoding_key),
                encoding_key: encoding_key.to_string(),
                entry,
            });
        }

        self.install_entries_parallel(
            cdn,
            archive_map,
            work,
            out_dir,
            options.verify,
            progress,
            cancel.clone(),
        )
        .await?;

        Ok(version)
    }

    async fn install_entries_parallel(
        &self,
        cdn: RemoteCdn,
        archive_map: archive::ArchiveMap,
        entries: Vec<InstallWorkItem>,
        out_dir: &Path,
        verify: bool,
        progress: &mut (impl FnMut(ProgressUpdate) + Send),
        cancel: Option<Arc<AtomicBool>>,
    ) -> Result<()> {
        let total_files = entries.len();
        let total_bytes = entries
            .iter()
            .map(|item| u64::from(item.entry.size))
            .sum::<u64>()
            .max(1);
        let (byte_sender, mut byte_receiver) = mpsc::unbounded_channel::<u64>();
        let install_cdn = cdn.with_progress_callback(Arc::new(move |bytes| {
            let _ = byte_sender.send(bytes);
        }));
        let mut pending = entries.into_iter();
        let mut tasks = JoinSet::new();
        let mut active = 0usize;
        let mut completed_files = 0usize;
        let mut completed_bytes = 0u64;
        let mut in_flight_bytes = 0u64;
        let mut speed_window_start = Instant::now();
        let mut speed_window_bytes = 0u64;
        let mut last_progress = Instant::now() - Duration::from_secs(1);

        loop {
            while active < INSTALL_FILE_CONCURRENCY {
                let Some(item) = pending.next() else {
                    break;
                };
                check_cancelled(cancel.as_ref())?;
                active += 1;
                let cdn = install_cdn.clone();
                let archive_map = archive_map.clone();
                let out_dir = out_dir.to_path_buf();
                tasks.spawn(async move {
                    install_one_entry(cdn, archive_map, item, out_dir, verify).await
                });
            }

            if active == 0 {
                break;
            }

            tokio::select! {
                Some(bytes) = byte_receiver.recv() => {
                    in_flight_bytes = in_flight_bytes.saturating_add(bytes);
                    speed_window_bytes = speed_window_bytes.saturating_add(bytes);
                    let elapsed = speed_window_start.elapsed();
                    if last_progress.elapsed() >= Duration::from_millis(250) {
                        let speed = if elapsed > Duration::ZERO {
                            speed_window_bytes as f64 / elapsed.as_secs_f64()
                        } else {
                            0.0
                        };
                        emit_install_progress(
                            progress,
                            completed_files,
                            total_files,
                            completed_bytes,
                            in_flight_bytes,
                            total_bytes,
                            speed,
                        );
                        if elapsed >= Duration::from_secs(1) {
                            speed_window_start = Instant::now();
                            speed_window_bytes = 0;
                        }
                        last_progress = Instant::now();
                    }
                }
                Some(result) = tasks.join_next() => {
                    active -= 1;
                    let installed_bytes = result??;
                    completed_files += 1;
                    completed_bytes = completed_bytes.saturating_add(installed_bytes);
                    in_flight_bytes = in_flight_bytes.saturating_sub(installed_bytes);
                    emit_install_progress(
                        progress,
                        completed_files,
                        total_files,
                        completed_bytes,
                        in_flight_bytes,
                        total_bytes,
                        0.0,
                    );
                }
            }
        }

        progress(ProgressUpdate::new("Installed Hearthstone files", 0.95));
        Ok(())
    }
}

fn emit_install_progress(
    progress: &mut (impl FnMut(ProgressUpdate) + Send),
    completed_files: usize,
    total_files: usize,
    completed_bytes: u64,
    in_flight_bytes: u64,
    total_bytes: u64,
    speed_bytes_per_second: f64,
) {
    let visible_bytes = completed_bytes
        .saturating_add(in_flight_bytes)
        .min(total_bytes);
    let fraction = visible_bytes as f64 / total_bytes as f64;
    let speed = if speed_bytes_per_second > 0.0 {
        format!(" at {}/s", format_bytes(speed_bytes_per_second))
    } else {
        String::new()
    };
    progress(ProgressUpdate::new(
        format!(
            "Downloading Hearthstone: {}/{} files, {}/{}{speed}",
            completed_files,
            total_files,
            format_bytes(visible_bytes as f64),
            format_bytes(total_bytes as f64)
        ),
        0.35 + fraction * 0.60,
    ));
}

async fn install_one_entry(
    cdn: RemoteCdn,
    archive_map: archive::ArchiveMap,
    item: InstallWorkItem,
    out_dir: PathBuf,
    verify: bool,
) -> Result<u64> {
    trace!(
        path = %item.entry.path,
        content_key = %item.entry.content_key,
        encoding_key = %item.encoding_key,
        size = item.entry.size,
        "installing entry"
    );
    let decoded = fetch_install_entry(
        &cdn,
        &archive_map,
        &item.encoding_key,
        &item.entry.content_key,
        &item.entry.path,
        verify,
        item.has_archive,
    )
    .await?;
    let target = out_dir.join(&item.entry.path);
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(target, decoded).await?;
    Ok(u64::from(item.entry.size))
}

async fn fetch_install_entry(
    cdn: &RemoteCdn,
    archive_map: &archive::ArchiveMap,
    encoding_key: &str,
    content_key: &str,
    path: &str,
    verify: bool,
    has_archive: bool,
) -> Result<Vec<u8>> {
    trace!(
        path = %path,
        encoding_key = %encoding_key,
        content_key = %content_key,
        has_archive = has_archive,
        "fetching install entry"
    );
    let loose_data = if has_archive {
        cdn.read_data_cache_unverified(encoding_key).await?
    } else {
        cdn.fetch_data_optional_unverified(encoding_key).await?
    };

    if let Some(encoded) = loose_data {
        if let Ok(decoded) = decode_install_entry(&encoded, encoding_key, content_key, path, verify)
        {
            trace!(
                path = %path,
                encoding_key = %encoding_key,
                bytes = decoded.len(),
                "decoded loose data"
            );
            cdn.cache_data(encoding_key, &encoded).await;
            return Ok(decoded);
        }
        warn!(
            path = %path,
            encoding_key = %encoding_key,
            content_key = %content_key,
            "loose data existed but failed decode/verification; removing cache and trying archive"
        );
        cdn.remove_data_cache(encoding_key).await;
    }

    anyhow::ensure!(
        has_archive,
        "loose data missing or invalid for {path} ({encoding_key})"
    );
    let decoded = archive_map
        .fetch_file(cdn, encoding_key, verify)
        .await
        .with_context(|| format!("archive data missing for {path}"))?;
    trace!(
        path = %path,
        encoding_key = %encoding_key,
        bytes = decoded.len(),
        "decoded archive data"
    );
    verify_md5("installed file", &decoded, content_key, verify)?;
    Ok(decoded)
}

impl NgdpClient {
    async fn fetch_encoding(
        &self,
        cdn: &RemoteCdn,
        build_config: &BuildConfig,
        verify: bool,
    ) -> Result<EncodingFile> {
        let pair = &build_config.encoding;
        anyhow::ensure!(
            !pair.content_key.is_empty(),
            "build config has no encoding content key"
        );
        anyhow::ensure!(
            !pair.encoding_key.is_empty(),
            "build config has no encoding data key"
        );
        let encoded = cdn.fetch_data(&pair.encoding_key, false).await?;
        debug!(encoding_key = %pair.encoding_key, bytes = encoded.len(), "fetched encoded encoding table");
        let decoded = blte::decode(&encoded, &pair.encoding_key, false)?;
        debug!(content_key = %pair.content_key, bytes = decoded.len(), "decoded encoding table");
        EncodingFile::parse(&decoded, &pair.content_key, verify)
    }

    async fn fetch_install_manifest(
        &self,
        cdn: &RemoteCdn,
        build_config: &BuildConfig,
        encoding: &EncodingFile,
        verify: bool,
    ) -> Result<InstallFile> {
        let install = &build_config.install;
        let encoding_key = if !install.encoding_key.is_empty() {
            install.encoding_key.clone()
        } else {
            encoding
                .find_by_content_key(&install.content_key)
                .context("install manifest encoding key not found")?
                .to_string()
        };
        let encoded = cdn.fetch_data(&encoding_key, false).await?;
        debug!(
            encoding_key = %encoding_key,
            bytes = encoded.len(),
            "fetched encoded install manifest"
        );
        let decoded = blte::decode(&encoded, &encoding_key, false)?;
        debug!(content_key = %install.content_key, bytes = decoded.len(), "decoded install manifest");
        InstallFile::parse(&decoded, &install.content_key, verify)
    }
}

fn check_cancelled(cancel: Option<&Arc<AtomicBool>>) -> Result<()> {
    if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
        warn!("NGDP install cancelled");
        anyhow::bail!("installation cancelled");
    }
    Ok(())
}

fn decode_install_entry(
    encoded: &[u8],
    encoding_key: &str,
    content_key: &str,
    path: &str,
    verify: bool,
) -> Result<Vec<u8>> {
    let decoded = blte::decode(encoded, encoding_key, false)
        .with_context(|| format!("failed to decode {path}"))?;
    verify_md5("installed file", &decoded, content_key, verify)
        .with_context(|| format!("failed to verify {path}"))?;
    Ok(decoded)
}

fn version_urls() -> Vec<String> {
    [Region::Us, Region::Eu, Region::Kr, Region::Cn]
        .into_iter()
        .map(|candidate| format!("{}/versions", candidate.remote_url()))
        .collect()
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

pub(crate) fn verify_md5(name: &str, data: &[u8], expected: &str, verify: bool) -> Result<()> {
    if verify {
        let actual = format!("{:x}", md5::compute(data));
        anyhow::ensure!(
            actual == expected,
            "{name} failed md5 verification: expected {expected}, got {actual}"
        );
    }
    Ok(())
}

pub(crate) fn partition_hash(hash: &str) -> Result<String> {
    anyhow::ensure!(hash.len() >= 4, "invalid hash `{hash}`");
    Ok(format!("{}/{}/{}", &hash[0..2], &hash[2..4], hash))
}

pub(crate) fn read_cstr(cursor: &mut std::io::Cursor<&[u8]>) -> Result<String> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if cursor.read(&mut byte)? == 0 || byte[0] == 0 {
            break;
        }
        out.push(byte[0]);
    }
    String::from_utf8(out).context("invalid utf-8 c-string")
}

pub(crate) fn read_be_u24(bytes: &[u8]) -> u32 {
    ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32
}
