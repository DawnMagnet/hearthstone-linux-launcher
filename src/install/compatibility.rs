use crate::{Locale, Region};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

pub fn install_compatibility_files(game_dir: &Path, region: Region, locale: Locale) -> Result<()> {
    info!(
        game_dir = %game_dir.display(),
        region = %region,
        locale = %locale,
        "installing compatibility files"
    );
    let config = include_str!("../../assets/client.config.in")
        .replace("{{aurora_env}}", region.aurora_env())
        .replace("{{locale}}", locale.as_str());
    std::fs::write(game_dir.join("client.config"), config)?;

    let stubs = stub_files()?;
    let frameworks = game_dir
        .join("Bin/Hearthstone_Data/Plugins/System/Library/Frameworks/CoreFoundation.framework");
    std::fs::create_dir_all(&frameworks)?;
    copy_required(
        &stubs.core_foundation,
        &frameworks.join("CoreFoundation.so"),
    )?;
    copy_required(
        &stubs.osx_window_management,
        &game_dir.join("Bin/Hearthstone_Data/Plugins/libOSXWindowManagement.so"),
    )?;
    copy_required(
        &stubs.blz_commerce_sdk_plugin,
        &game_dir.join("Bin/Hearthstone_Data/Plugins/libblz_commerce_sdk_plugin.so"),
    )?;
    copy_required(
        &stubs.commerce_http_client,
        &game_dir.join("Bin/Hearthstone_Data/Plugins/libcommerce_http_client.so"),
    )?;
    copy_required(
        &stubs.commerce_http_client,
        &game_dir.join("Bin/Hearthstone_Data/MonoBleedingEdge/x86_64/libcommerce_http_client.so"),
    )?;
    Ok(())
}

struct StubFiles {
    core_foundation: PathBuf,
    osx_window_management: PathBuf,
    blz_commerce_sdk_plugin: PathBuf,
    commerce_http_client: PathBuf,
}

fn copy_required(from: &Path, to: &Path) -> Result<()> {
    debug!(from = %from.display(), to = %to.display(), "copying compatibility file");
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(from, to)
        .with_context(|| format!("failed to copy {} to {}", from.display(), to.display()))?;
    Ok(())
}

fn resource_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("HEARTHSTONE_LINUX_RESOURCES") {
        debug!(path = %path, "using resources from environment");
        return Ok(PathBuf::from(path));
    }

    let exe = std::env::current_exe()?;
    if let Some(prefix) = exe.parent().and_then(|bin| bin.parent()) {
        let share = prefix.join("share/hearthstone-linux-gui");
        if share.exists() {
            debug!(path = %share.display(), "using installed resources");
            return Ok(share);
        }
    }

    anyhow::bail!("could not locate hearthstone-linux-gui resources")
}

fn stub_files() -> Result<StubFiles> {
    if let Ok(path) = std::env::var("HEARTHSTONE_LINUX_STUBS") {
        debug!(path = %path, "using stubs from environment");
        return stub_files_in(PathBuf::from(path));
    }

    if let Ok(resources) = resource_dir() {
        let stubs = resources.join("stubs");
        if stubs.exists() {
            debug!(path = %stubs.display(), "using installed stubs");
            return stub_files_in(stubs);
        }
    }

    if let Some(stubs) = dev_stub_files()? {
        debug!("using development stub libraries");
        return Ok(stubs);
    }

    anyhow::bail!("could not locate hearthstone-linux-gui stub libraries")
}

fn stub_files_in(dir: PathBuf) -> Result<StubFiles> {
    let files = StubFiles {
        core_foundation: dir.join("CoreFoundation.so"),
        osx_window_management: dir.join("libOSXWindowManagement.so"),
        blz_commerce_sdk_plugin: dir.join("libblz_commerce_sdk_plugin.so"),
        commerce_http_client: dir.join("libcommerce_http_client.so"),
    };
    if files.core_foundation.exists()
        && files.osx_window_management.exists()
        && files.blz_commerce_sdk_plugin.exists()
        && files.commerce_http_client.exists()
    {
        return Ok(files);
    }

    anyhow::bail!("stub libraries are missing from {}", dir.display())
}

fn dev_stub_files() -> Result<Option<StubFiles>> {
    let exe = std::env::current_exe()?;
    let Some(profile_dir) = exe.parent() else {
        return Ok(None);
    };

    for dir in [profile_dir, &profile_dir.join("deps")] {
        let files = StubFiles {
            core_foundation: dir.join("libCoreFoundation.so"),
            osx_window_management: dir.join("libOSXWindowManagement.so"),
            blz_commerce_sdk_plugin: dir.join("libblz_commerce_sdk_plugin.so"),
            commerce_http_client: dir.join("libcommerce_http_client.so"),
        };
        if files.core_foundation.exists()
            && files.osx_window_management.exists()
            && files.blz_commerce_sdk_plugin.exists()
            && files.commerce_http_client.exists()
        {
            return Ok(Some(files));
        }
    }

    Ok(None)
}
