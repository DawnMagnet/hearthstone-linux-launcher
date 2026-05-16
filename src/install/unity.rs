use anyhow::{Context, Result};
use reqwest::{
    header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE},
    StatusCode,
};
use serde_json::json;
use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};

const UNITY_ENGINE: &str =
    "Editor/Data/PlaybackEngines/LinuxStandaloneSupport/Variations/linux64_player_nondevelopment_mono";

pub async fn ensure_unity_player(game_dir: &Path, cache_dir: &Path) -> Result<String> {
    ensure_unity_player_with_progress(game_dir, cache_dir, None, |_| {}).await
}

#[derive(Clone, Debug)]
pub struct UnityDownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bytes_per_second: f64,
    pub resumed: bool,
}

impl UnityDownloadProgress {
    pub fn fraction(&self) -> Option<f64> {
        let total = self.total?;
        if total == 0 {
            return None;
        }
        Some((self.downloaded as f64 / total as f64).clamp(0.0, 1.0))
    }
}

pub async fn ensure_unity_player_with_progress(
    game_dir: &Path,
    cache_dir: &Path,
    cancel: Option<Arc<AtomicBool>>,
    mut progress: impl FnMut(UnityDownloadProgress) + Send,
) -> Result<String> {
    let unity_version = detect_unity_version(&game_dir.join("Bin/Hearthstone_Data/level0"))?;
    info!(version = %unity_version, "detected required Unity version");
    let marker = game_dir.join(".unity");
    if marker.exists()
        && std::fs::read_to_string(&marker).unwrap_or_default().trim() == unity_version
        && unity_player_files_exist(game_dir)
    {
        debug!(version = %unity_version, "Unity player already installed");
        return Ok(unity_version);
    }

    std::fs::create_dir_all(cache_dir)?;
    let unity_root = cache_dir.join(&unity_version).join(UNITY_ENGINE);
    if !unity_root.join("LinuxPlayer").exists() {
        info!(version = %unity_version, cache_dir = %cache_dir.display(), "Unity player cache miss");
        download_unity(&unity_version, cache_dir, cancel.as_ref(), &mut progress).await?;
    }

    copy_unity_files(&unity_root, game_dir)?;
    std::fs::write(marker, &unity_version)?;
    Ok(unity_version)
}

fn detect_unity_version(level0: &Path) -> Result<String> {
    let data =
        std::fs::read(level0).with_context(|| format!("failed to read {}", level0.display()))?;
    let mut best = Vec::new();
    for byte in data {
        if byte.is_ascii_graphic() || byte == b' ' {
            best.push(byte);
        } else if !best.is_empty() {
            let text = String::from_utf8_lossy(&best);
            if looks_like_unity_version(&text) {
                return Ok(text.trim().to_string());
            }
            best.clear();
        }
    }
    anyhow::bail!(
        "could not determine Unity version from {}",
        level0.display()
    )
}

fn looks_like_unity_version(value: &str) -> bool {
    let value = value.trim();
    value.len() >= 6
        && value.contains('.')
        && value.bytes().any(|byte| byte == b'f' || byte == b'p')
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_digit())
}

async fn download_unity(
    version: &str,
    cache_dir: &Path,
    cancel: Option<&Arc<AtomicBool>>,
    progress: &mut (impl FnMut(UnityDownloadProgress) + Send),
) -> Result<()> {
    let hash = fetch_unity_archive_hash(version).await?;
    let url = format!(
        "https://download.unity3d.com/download_unity/{hash}/LinuxEditorInstaller/Unity.tar.xz"
    );
    let archive_path = cache_dir.join(format!("{version}.tar.xz"));
    info!(
        version = %version,
        url = %url,
        archive = %archive_path.display(),
        "downloading Unity archive"
    );
    download_file_resumable(&url, &archive_path, cancel, progress).await?;
    debug!(archive = %archive_path.display(), "extracting Unity archive");
    extract_unity_archive(&archive_path, &cache_dir.join(version))?;
    Ok(())
}

async fn download_file_resumable(
    url: &str,
    destination: &Path,
    cancel: Option<&Arc<AtomicBool>>,
    progress: &mut (impl FnMut(UnityDownloadProgress) + Send),
) -> Result<()> {
    check_cancelled(cancel)?;
    if destination.exists() {
        let downloaded = tokio::fs::metadata(destination).await?.len();
        progress(UnityDownloadProgress {
            downloaded,
            total: Some(downloaded),
            speed_bytes_per_second: 0.0,
            resumed: false,
        });
        return Ok(());
    }

    let partial = partial_download_path(destination);
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .build()?;
    let expected_total = fetch_content_length(&client, url).await.unwrap_or(None);
    let mut offset = file_len(&partial).await?;
    if let Some(total) = expected_total {
        if offset == total {
            tokio::fs::rename(&partial, destination).await?;
            progress(UnityDownloadProgress {
                downloaded: total,
                total: Some(total),
                speed_bytes_per_second: 0.0,
                resumed: true,
            });
            return Ok(());
        }
        if offset > total {
            warn!(
                partial = %partial.display(),
                partial_bytes = offset,
                total_bytes = total,
                "partial Unity archive is larger than remote file; restarting download"
            );
            let _ = tokio::fs::remove_file(&partial).await;
            offset = 0;
        }
    }

    for _ in 1..=2 {
        check_cancelled(cancel)?;
        let mut request = client.get(url);
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }

        let response = request.send().await?;
        let status = response.status();
        if status == StatusCode::RANGE_NOT_SATISFIABLE && offset > 0 {
            warn!(
                partial = %partial.display(),
                offset = offset,
                "remote rejected resume range; restarting Unity download"
            );
            let _ = tokio::fs::remove_file(&partial).await;
            offset = 0;
            continue;
        }

        let response = response.error_for_status()?;
        let status = response.status();
        let append = status == StatusCode::PARTIAL_CONTENT && offset > 0;
        if offset > 0 && !append {
            warn!("Unity download server ignored range request; restarting download");
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
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&partial)
            .await?;
        let resumed = append;
        progress(UnityDownloadProgress {
            downloaded,
            total,
            speed_bytes_per_second: 0.0,
            resumed,
        });

        let mut response = response;
        let mut window_start = Instant::now();
        let mut window_bytes = 0u64;
        let idle_timeout = Duration::from_secs(30);
        loop {
            check_cancelled(cancel)?;
            match tokio::time::timeout(idle_timeout, response.chunk()).await {
                Ok(Ok(Some(chunk))) => {
                    file.write_all(&chunk).await?;
                    let chunk_len = chunk.len() as u64;
                    downloaded += chunk_len;
                    window_bytes += chunk_len;
                    let elapsed = window_start.elapsed();
                    if elapsed >= Duration::from_millis(500) {
                        progress(UnityDownloadProgress {
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
                    "Unity download stalled, no data received for {}s",
                    idle_timeout.as_secs()
                ),
            }
        }
        file.flush().await?;

        if let Some(total) = total {
            anyhow::ensure!(
                downloaded >= total,
                "Unity archive download was incomplete: got {downloaded} of {total} bytes"
            );
        }
        tokio::fs::rename(&partial, destination).await?;
        progress(UnityDownloadProgress {
            downloaded,
            total: total.or(Some(downloaded)),
            speed_bytes_per_second: 0.0,
            resumed,
        });
        return Ok(());
    }

    anyhow::bail!("failed to resume Unity download after retrying from the beginning")
}

async fn fetch_content_length(client: &reqwest::Client, url: &str) -> Result<Option<u64>> {
    let response = client.head(url).send().await?.error_for_status()?;
    Ok(response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok()))
}

async fn file_len(path: &Path) -> Result<u64> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
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

fn check_cancelled(cancel: Option<&Arc<AtomicBool>>) -> Result<()> {
    if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
        warn!("Unity download cancelled");
        anyhow::bail!("installation cancelled");
    }
    Ok(())
}

async fn fetch_unity_archive_hash(version: &str) -> Result<String> {
    debug!(version = %version, "fetching Unity archive hash");
    let body = json!({
        "operationName": "GetRelease",
        "variables": {
            "version": version,
            "limit": 300
        },
        "query": "query GetRelease($limit: Int, $skip: Int, $version: String!, $stream: [UnityReleaseStream!]) { getUnityReleases(limit: $limit skip: $skip stream: $stream version: $version entitlements: [XLTS]) { totalCount edges { node { version entitlements releaseDate unityHubDeepLink stream __typename } __typename } __typename } }"
    });
    let text = reqwest::Client::new()
        .post("https://services.unity.com/graphql")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let needle = format!("unityhub://{version}/");
    let start = text
        .find(&needle)
        .with_context(|| format!("Unity {version} was not found in Unity release archive"))?
        + needle.len();
    let hash: String = text[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric())
        .collect();
    anyhow::ensure!(!hash.is_empty(), "Unity archive hash was empty");
    Ok(hash)
}

fn extract_unity_archive(archive_path: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    let file = File::open(archive_path)?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        if should_extract(&path) {
            let target = destination.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            entry.unpack(target)?;
        }
    }
    Ok(())
}

fn should_extract(path: &Path) -> bool {
    let path = path.to_string_lossy();
    path == format!("{UNITY_ENGINE}/LinuxPlayer")
        || path == format!("{UNITY_ENGINE}/UnityPlayer.so")
        || path.starts_with(&format!("{UNITY_ENGINE}/Data/MonoBleedingEdge/"))
}

fn copy_unity_files(unity_root: &Path, game_dir: &Path) -> Result<()> {
    info!(unity_root = %unity_root.display(), game_dir = %game_dir.display(), "copying Unity player files");
    let bin = game_dir.join("Bin");
    let data = bin.join("Hearthstone_Data");
    std::fs::create_dir_all(&bin)?;
    std::fs::copy(
        unity_root.join("LinuxPlayer"),
        bin.join("Hearthstone.x86_64"),
    )?;
    std::fs::copy(
        unity_root.join("UnityPlayer.so"),
        bin.join("UnityPlayer.so"),
    )?;
    copy_dir(
        &unity_root.join("Data/MonoBleedingEdge"),
        &data.join("MonoBleedingEdge"),
    )?;
    make_executable(&bin.join("Hearthstone.x86_64"))?;
    Ok(())
}

fn unity_player_files_exist(game_dir: &Path) -> bool {
    let bin = game_dir.join("Bin");
    let data = bin.join("Hearthstone_Data");
    bin.join("Hearthstone.x86_64").exists()
        && bin.join("UnityPlayer.so").exists()
        && data
            .join("MonoBleedingEdge/x86_64/libmonobdwgc-2.0.so")
            .exists()
        && data.join("MonoBleedingEdge/etc/mono/config").exists()
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    if to.exists() {
        std::fs::remove_dir_all(to)?;
    }
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(from)?;
        let target = to.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}
