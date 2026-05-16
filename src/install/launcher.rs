use anyhow::{Context, Result};
use std::{
    collections::HashSet,
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{Child, Command},
};
use tracing::{debug, info, warn};

pub fn launch_game(game_dir: &Path) -> Result<Child> {
    let exe = game_dir.join("Bin/Hearthstone.x86_64");
    anyhow::ensure!(exe.exists(), "{} does not exist", exe.display());
    anyhow::ensure!(
        game_dir.join("Bin/UnityPlayer.so").exists(),
        "UnityPlayer.so is missing; run Install / Update to repair the Unity runtime"
    );
    anyhow::ensure!(
        game_dir
            .join("Bin/Hearthstone_Data/MonoBleedingEdge/x86_64/libmonobdwgc-2.0.so")
            .exists(),
        "Unity Mono runtime is missing; run Install / Update to repair the Unity runtime"
    );
    anyhow::ensure!(game_dir.join("token").exists(), "login token is missing");
    anyhow::ensure!(
        game_dir.join("client.config").exists(),
        "client.config is missing"
    );

    info!(exe = %exe.display(), game_dir = %game_dir.display(), "launching Hearthstone");
    if let Some(runner) = find_fhs_runner() {
        info!(runner = %runner.display(), "launching Hearthstone through FHS runtime");
        return Command::new(&runner)
            .arg(&exe)
            .current_dir(game_dir)
            .spawn()
            .with_context(|| format!("failed to launch Hearthstone through {}", runner.display()));
    }

    warn!("steam-run was not found; falling back to direct launch");
    let library_path = game_library_path(game_dir);
    debug!(ld_library_path = ?library_path, "configured game library path");
    let child = Command::new(exe)
        .current_dir(game_dir)
        .env("LD_LIBRARY_PATH", library_path)
        .envs(graphics_env())
        .spawn()
        .context("failed to launch Hearthstone")?;
    Ok(child)
}

fn game_library_path(game_dir: &Path) -> OsString {
    let mut paths = Vec::from([
        game_dir.join("Bin"),
        game_dir.join("Bin/Hearthstone_Data/Plugins"),
        game_dir.join(
            "Bin/Hearthstone_Data/Plugins/System/Library/Frameworks/CoreFoundation.framework",
        ),
        game_dir.join("Bin/Hearthstone_Data/MonoBleedingEdge/x86_64"),
    ]);
    push_existing(&mut paths, "/run/opengl-driver/lib");
    push_existing(&mut paths, "/run/current-system/sw/share/nix-ld/lib");
    push_existing(&mut paths, "/run/current-system/sw/lib");
    paths.extend(nix_ld_library_paths());
    if let Some(existing) = env::var_os("NIX_LD_LIBRARY_PATH") {
        paths.extend(env::split_paths(&existing));
    }
    if let Some(existing) = env::var_os("LD_LIBRARY_PATH") {
        paths.extend(env::split_paths(&existing));
    }

    dedupe_paths(&mut paths);
    std::env::join_paths(paths).unwrap_or_default()
}

fn nix_ld_library_paths() -> Vec<PathBuf> {
    let Some(flags) = env::var_os("NIX_LDFLAGS") else {
        return Vec::new();
    };
    flags
        .to_string_lossy()
        .split_whitespace()
        .filter_map(|flag| flag.strip_prefix("-L").map(PathBuf::from))
        .collect()
}

fn find_fhs_runner() -> Option<PathBuf> {
    if env::var_os("HEARTHSTONE_LINUX_DIRECT_LAUNCH").is_some() {
        return None;
    }
    if let Some(runner) = env::var_os("HEARTHSTONE_LINUX_RUNNER") {
        let runner = PathBuf::from(runner);
        if runner.exists() {
            return Some(runner);
        }
    }

    find_in_path("steam-run").or_else(|| {
        let runner = PathBuf::from("/run/current-system/sw/bin/steam-run");
        runner.exists().then_some(runner)
    })
}

fn find_in_path(command: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH")?).find_map(|dir| {
        let path = dir.join(command);
        path.exists().then_some(path)
    })
}

fn graphics_env() -> Vec<(&'static str, OsString)> {
    let mut envs = Vec::new();
    push_env_path(
        &mut envs,
        "LIBGL_DRIVERS_PATH",
        OsStr::new("/run/opengl-driver/lib/dri"),
    );
    push_env_path(
        &mut envs,
        "__EGL_VENDOR_LIBRARY_DIRS",
        OsStr::new("/run/opengl-driver/share/glvnd/egl_vendor.d"),
    );
    envs
}

fn push_env_path(envs: &mut Vec<(&'static str, OsString)>, name: &'static str, path: &OsStr) {
    let path = Path::new(path);
    if !path.exists() {
        return;
    }

    let mut paths = vec![path.to_path_buf()];
    if let Some(existing) = env::var_os(name) {
        paths.extend(env::split_paths(&existing));
    }
    dedupe_paths(&mut paths);
    if let Ok(joined) = env::join_paths(paths) {
        envs.push((name, joined));
    }
}

fn push_existing(paths: &mut Vec<PathBuf>, path: impl AsRef<Path>) {
    let path = path.as_ref();
    if path.exists() {
        paths.push(path.to_path_buf());
    }
}

fn dedupe_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path.clone()));
}
