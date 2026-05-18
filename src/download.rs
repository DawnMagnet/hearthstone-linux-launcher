use anyhow::{Context, Result};
use reqwest::{
    header::{ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE, RANGE},
    StatusCode, Url,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
    task::JoinSet,
};
use tracing::{debug, warn};

const CHUNK_SIZE: u64 = 8 * 1024 * 1024;
const PARALLEL_THRESHOLD: u64 = 16 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 8;
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bytes_per_second: f64,
    pub resumed: bool,
}

impl DownloadProgress {
    pub fn fraction(&self) -> Option<f64> {
        let total = self.total?;
        if total == 0 {
            return None;
        }
        Some((self.downloaded as f64 / total as f64).clamp(0.0, 1.0))
    }
}

#[derive(Clone, Copy, Debug)]
struct RemoteFile {
    total: Option<u64>,
    ranges: bool,
}

#[derive(Clone, Debug)]
struct DownloadChunk {
    index: usize,
    start: u64,
    end: u64,
}

#[derive(Debug)]
struct ChunkResult {
    index: usize,
    data: Vec<u8>,
}

pub async fn download_to_vec(
    client: &reqwest::Client,
    url: Url,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<dyn Fn(u64) + Send + Sync>>,
) -> Result<Vec<u8>> {
    check_cancelled(cancel.as_ref())?;
    let remote = probe_remote(client, url.clone())
        .await
        .unwrap_or(RemoteFile {
            total: None,
            ranges: false,
        });

    if remote.ranges
        && remote
            .total
            .is_some_and(|total| total >= PARALLEL_THRESHOLD)
    {
        match download_to_vec_parallel(
            client,
            url.clone(),
            remote.total.unwrap(),
            cancel.clone(),
            progress.clone(),
        )
        .await
        {
            Ok(data) => return Ok(data),
            Err(error) => {
                warn!(url = %url, error = %format!("{error:#}"), "parallel download failed; retrying with a single connection");
            }
        }
    }

    download_to_vec_single(client, url, cancel, progress).await
}

pub async fn download_range_to_vec(
    client: &reqwest::Client,
    url: Url,
    offset: u64,
    size: u64,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<dyn Fn(u64) + Send + Sync>>,
) -> Result<Vec<u8>> {
    check_cancelled(cancel.as_ref())?;
    if size == 0 {
        return Ok(Vec::new());
    }

    let end = offset
        .checked_add(size)
        .and_then(|value| value.checked_sub(1))
        .context("download range overflows")?;
    let response = client
        .get(url.clone())
        .header(RANGE, format!("bytes={offset}-{end}"))
        .send()
        .await?;
    anyhow::ensure!(
        response.status() == StatusCode::PARTIAL_CONTENT,
        "server did not honor range request for {url}"
    );
    let mut response = response.error_for_status()?;
    let mut data = Vec::with_capacity(size as usize);

    loop {
        check_cancelled(cancel.as_ref())?;
        match tokio::time::timeout(IDLE_TIMEOUT, response.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                notify_progress(&progress, chunk.len() as u64);
                data.extend_from_slice(&chunk);
            }
            Ok(Ok(None)) => break,
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => anyhow::bail!(
                "connection stalled, no data received for {}s",
                IDLE_TIMEOUT.as_secs()
            ),
        }
    }

    anyhow::ensure!(
        data.len() as u64 == size,
        "range download returned {} bytes, expected {size}",
        data.len()
    );
    Ok(data)
}

pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    cancel: Option<Arc<AtomicBool>>,
    mut progress: impl FnMut(DownloadProgress) + Send,
) -> Result<()> {
    check_cancelled(cancel.as_ref())?;
    if destination.exists() {
        let downloaded = tokio::fs::metadata(destination).await?.len();
        progress(DownloadProgress {
            downloaded,
            total: Some(downloaded),
            speed_bytes_per_second: 0.0,
            resumed: false,
        });
        return Ok(());
    }

    let url = Url::parse(url)?;
    let remote = probe_remote(client, url.clone())
        .await
        .unwrap_or(RemoteFile {
            total: None,
            ranges: false,
        });

    if remote.ranges
        && remote
            .total
            .is_some_and(|total| total >= PARALLEL_THRESHOLD)
    {
        download_file_parallel(
            client,
            url,
            remote.total.unwrap(),
            destination,
            cancel,
            progress,
        )
        .await
    } else {
        download_file_single(client, url, remote.total, destination, cancel, progress).await
    }
}

async fn probe_remote(client: &reqwest::Client, url: Url) -> Result<RemoteFile> {
    let response = client.head(url).send().await?.error_for_status()?;
    let total = response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok());
    let ranges = response
        .headers()
        .get(ACCEPT_RANGES)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("bytes"));

    Ok(RemoteFile { total, ranges })
}

async fn download_to_vec_parallel(
    client: &reqwest::Client,
    url: Url,
    total: u64,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<dyn Fn(u64) + Send + Sync>>,
) -> Result<Vec<u8>> {
    let chunks = chunks_for_total(total);
    let mut pending = chunks.iter();
    let mut tasks = JoinSet::new();
    let mut active = 0usize;
    let mut results = Vec::with_capacity(chunks.len());

    loop {
        while active < MAX_CONNECTIONS {
            let Some(chunk) = pending.next() else {
                break;
            };
            active += 1;
            let client = client.clone();
            let url = url.clone();
            let cancel = cancel.clone();
            let progress = progress.clone();
            let chunk = chunk.clone();
            tasks.spawn(async move {
                download_chunk_to_vec(&client, url, &chunk, cancel, progress)
                    .await
                    .map(|data| ChunkResult {
                        index: chunk.index,
                        data,
                    })
            });
        }

        if active == 0 {
            break;
        }

        let result = tasks
            .join_next()
            .await
            .context("parallel download workers stopped unexpectedly")??;
        let result = result?;
        active -= 1;
        results.push(result);
    }

    results.sort_by_key(|result| result.index);
    let mut data = Vec::with_capacity(total as usize);
    for result in results {
        data.extend_from_slice(&result.data);
    }
    anyhow::ensure!(
        data.len() as u64 == total,
        "parallel download returned {} bytes, expected {total}",
        data.len()
    );
    Ok(data)
}

async fn download_to_vec_single(
    client: &reqwest::Client,
    url: Url,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<dyn Fn(u64) + Send + Sync>>,
) -> Result<Vec<u8>> {
    let mut response = client.get(url.clone()).send().await?.error_for_status()?;
    let total = response.content_length();
    let mut data = Vec::with_capacity(total.unwrap_or(0) as usize);
    loop {
        check_cancelled(cancel.as_ref())?;
        match tokio::time::timeout(IDLE_TIMEOUT, response.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                notify_progress(&progress, chunk.len() as u64);
                data.extend_from_slice(&chunk);
            }
            Ok(Ok(None)) => break,
            Ok(Err(error)) => return Err(error.into()),
            Err(_) => anyhow::bail!(
                "connection stalled, no data received for {}s",
                IDLE_TIMEOUT.as_secs()
            ),
        }
    }
    debug!(url = %url, bytes = data.len(), "downloaded URL with one connection");
    Ok(data)
}

async fn download_file_parallel(
    client: &reqwest::Client,
    url: Url,
    total: u64,
    destination: &Path,
    cancel: Option<Arc<AtomicBool>>,
    mut progress: impl FnMut(DownloadProgress) + Send,
) -> Result<()> {
    let chunk_dir = chunk_download_dir(destination);
    tokio::fs::create_dir_all(&chunk_dir).await?;
    let chunks = chunks_for_total(total);
    let mut downloaded = completed_chunk_bytes(&chunk_dir, &chunks).await?;
    let resumed = downloaded > 0;
    progress(DownloadProgress {
        downloaded,
        total: Some(total),
        speed_bytes_per_second: 0.0,
        resumed,
    });

    let (progress_sender, mut progress_receiver) = mpsc::unbounded_channel();
    let mut pending = chunks
        .iter()
        .filter(|chunk| !chunk_path(&chunk_dir, chunk.index).exists());
    let mut tasks = JoinSet::new();
    let mut active = 0usize;
    let mut window_start = Instant::now();
    let mut window_bytes = 0u64;

    loop {
        while active < MAX_CONNECTIONS {
            let Some(chunk) = pending.next() else {
                break;
            };
            active += 1;
            let client = client.clone();
            let url = url.clone();
            let cancel = cancel.clone();
            let progress_sender = progress_sender.clone();
            let chunk = chunk.clone();
            let path = chunk_path(&chunk_dir, chunk.index);
            tasks.spawn(async move {
                let progress = Arc::new(move |bytes| {
                    let _ = progress_sender.send(bytes);
                });
                download_chunk_to_file(&client, url, &chunk, &path, cancel, Some(progress))
                    .await
                    .map(|bytes| (chunk.index, bytes))
            });
        }

        if active == 0 {
            break;
        }

        tokio::select! {
            Some(bytes) = progress_receiver.recv() => {
                downloaded = downloaded.saturating_add(bytes).min(total);
                window_bytes = window_bytes.saturating_add(bytes);
                let elapsed = window_start.elapsed();
                if elapsed >= Duration::from_millis(500) || downloaded >= total {
                    progress(DownloadProgress {
                        downloaded,
                        total: Some(total),
                        speed_bytes_per_second: window_bytes as f64 / elapsed.as_secs_f64().max(0.001),
                        resumed,
                    });
                    window_start = Instant::now();
                    window_bytes = 0;
                }
            }
            Some(result) = tasks.join_next() => {
                result.context("parallel file download workers stopped unexpectedly")??;
                active -= 1;
            }
        }
    }

    assemble_chunks(&chunk_dir, &chunks, destination).await?;
    let _ = tokio::fs::remove_dir_all(&chunk_dir).await;
    progress(DownloadProgress {
        downloaded: total,
        total: Some(total),
        speed_bytes_per_second: 0.0,
        resumed,
    });
    Ok(())
}

async fn download_file_single(
    client: &reqwest::Client,
    url: Url,
    expected_total: Option<u64>,
    destination: &Path,
    cancel: Option<Arc<AtomicBool>>,
    mut progress: impl FnMut(DownloadProgress) + Send,
) -> Result<()> {
    let partial = partial_download_path(destination);
    let mut offset = file_len(&partial).await?;
    if let Some(total) = expected_total {
        if offset == total {
            tokio::fs::rename(&partial, destination).await?;
            progress(DownloadProgress {
                downloaded: total,
                total: Some(total),
                speed_bytes_per_second: 0.0,
                resumed: true,
            });
            return Ok(());
        }
        if offset > total {
            let _ = tokio::fs::remove_file(&partial).await;
            offset = 0;
        }
    }

    for _ in 1..=2 {
        check_cancelled(cancel.as_ref())?;
        let mut request = client.get(url.clone());
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }

        let response = request.send().await?;
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE && offset > 0 {
            let _ = tokio::fs::remove_file(&partial).await;
            offset = 0;
            continue;
        }

        let response = response.error_for_status()?;
        let append = response.status() == StatusCode::PARTIAL_CONTENT && offset > 0;
        if offset > 0 && !append {
            offset = 0;
        }

        let total = if append {
            content_range_total(response.headers())
                .or_else(|| {
                    response
                        .content_length()
                        .map(|remaining| offset + remaining)
                })
                .or(expected_total)
        } else {
            response.content_length().or(expected_total)
        };
        let mut downloaded = if append { offset } else { 0 };
        let resumed = append;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&partial)
            .await?;
        let mut response = response;
        let mut window_start = Instant::now();
        let mut window_bytes = 0u64;
        progress(DownloadProgress {
            downloaded,
            total,
            speed_bytes_per_second: 0.0,
            resumed,
        });

        loop {
            check_cancelled(cancel.as_ref())?;
            match tokio::time::timeout(IDLE_TIMEOUT, response.chunk()).await {
                Ok(Ok(Some(chunk))) => {
                    file.write_all(&chunk).await?;
                    let chunk_len = chunk.len() as u64;
                    downloaded += chunk_len;
                    window_bytes += chunk_len;
                    let elapsed = window_start.elapsed();
                    if elapsed >= Duration::from_millis(500) {
                        progress(DownloadProgress {
                            downloaded,
                            total,
                            speed_bytes_per_second: window_bytes as f64 / elapsed.as_secs_f64(),
                            resumed,
                        });
                        window_start = Instant::now();
                        window_bytes = 0;
                    }
                }
                Ok(Ok(None)) => break,
                Ok(Err(error)) => return Err(error.into()),
                Err(_) => anyhow::bail!(
                    "connection stalled, no data received for {}s",
                    IDLE_TIMEOUT.as_secs()
                ),
            }
        }
        file.flush().await?;

        if let Some(total) = total {
            anyhow::ensure!(
                downloaded >= total,
                "download was incomplete: got {downloaded} of {total} bytes"
            );
        }
        tokio::fs::rename(&partial, destination).await?;
        progress(DownloadProgress {
            downloaded,
            total: total.or(Some(downloaded)),
            speed_bytes_per_second: 0.0,
            resumed,
        });
        return Ok(());
    }

    anyhow::bail!("failed to resume download after retrying from the beginning")
}

async fn download_chunk_to_vec(
    client: &reqwest::Client,
    url: Url,
    chunk: &DownloadChunk,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<dyn Fn(u64) + Send + Sync>>,
) -> Result<Vec<u8>> {
    download_range_to_vec(
        client,
        url,
        chunk.start,
        chunk.end - chunk.start + 1,
        cancel,
        progress,
    )
    .await
}

async fn download_chunk_to_file(
    client: &reqwest::Client,
    url: Url,
    chunk: &DownloadChunk,
    path: &Path,
    cancel: Option<Arc<AtomicBool>>,
    progress: Option<Arc<dyn Fn(u64) + Send + Sync>>,
) -> Result<u64> {
    check_cancelled(cancel.as_ref())?;
    let data = download_range_to_vec(
        client,
        url,
        chunk.start,
        chunk.end - chunk.start + 1,
        cancel,
        progress,
    )
    .await?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, &data).await?;
    Ok(data.len() as u64)
}

async fn assemble_chunks(
    chunk_dir: &Path,
    chunks: &[DownloadChunk],
    destination: &Path,
) -> Result<()> {
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let partial = partial_download_path(destination);
    let mut output = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&partial)
        .await?;

    for chunk in chunks {
        let path = chunk_path(chunk_dir, chunk.index);
        let mut input = tokio::fs::File::open(&path).await?;
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            let read = input.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read]).await?;
        }
    }
    output.flush().await?;
    tokio::fs::rename(partial, destination).await?;
    Ok(())
}

async fn completed_chunk_bytes(chunk_dir: &Path, chunks: &[DownloadChunk]) -> Result<u64> {
    let mut total = 0;
    for chunk in chunks {
        let path = chunk_path(chunk_dir, chunk.index);
        let expected = chunk.end - chunk.start + 1;
        match tokio::fs::metadata(&path).await {
            Ok(metadata) if metadata.len() == expected => total += expected,
            Ok(_) => {
                let _ = tokio::fs::remove_file(path).await;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(total)
}

fn chunks_for_total(total: u64) -> Vec<DownloadChunk> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < total {
        let end = (start + CHUNK_SIZE - 1).min(total - 1);
        chunks.push(DownloadChunk {
            index: chunks.len(),
            start,
            end,
        });
        start = end + 1;
    }
    chunks
}

fn notify_progress(progress: &Option<Arc<dyn Fn(u64) + Send + Sync>>, bytes: u64) {
    if let Some(progress) = progress {
        progress(bytes);
    }
}

fn content_range_total(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let value = headers.get(CONTENT_RANGE)?.to_str().ok()?;
    value.rsplit('/').next()?.parse().ok()
}

fn partial_download_path(destination: &Path) -> PathBuf {
    let mut partial = destination.to_path_buf();
    let extension = destination
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!("{extension}.part"))
        .unwrap_or_else(|| "part".to_string());
    partial.set_extension(extension);
    partial
}

fn chunk_download_dir(destination: &Path) -> PathBuf {
    let mut dir = destination.to_path_buf();
    let extension = destination
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!("{extension}.parts"))
        .unwrap_or_else(|| "parts".to_string());
    dir.set_extension(extension);
    dir
}

fn chunk_path(chunk_dir: &Path, index: usize) -> PathBuf {
    chunk_dir.join(format!("{index:06}.part"))
}

async fn file_len(path: &Path) -> Result<u64> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

fn check_cancelled(cancel: Option<&Arc<AtomicBool>>) -> Result<()> {
    if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
        anyhow::bail!("download cancelled");
    }
    Ok(())
}
