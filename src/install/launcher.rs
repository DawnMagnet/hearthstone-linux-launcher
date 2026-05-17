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
    ensure_bundled_interpreter(&exe)?;

    info!(exe = %exe.display(), game_dir = %game_dir.display(), "launching Hearthstone");
    let library_path = game_library_path(game_dir);
    debug!(ld_library_path = ?library_path, "configured game library path");
    if let Some(runner) = find_runtime_runner() {
        info!(runner = %runner.display(), "launching Hearthstone through runtime");
        return Command::new(&runner)
            .arg(&exe)
            .current_dir(game_dir)
            .env("LD_LIBRARY_PATH", library_path)
            .envs(graphics_env())
            .spawn()
            .with_context(|| format!("failed to launch Hearthstone through {}", runner.display()));
    }

    debug!("no runtime runner configured; launching directly");
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
    paths.extend(runtime_library_paths());
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

fn runtime_library_paths() -> Vec<PathBuf> {
    let Some(runtime_dir) = env::var_os("HEARTHSTONE_LINUX_RUNTIME_DIR") else {
        return Vec::new();
    };

    env::split_paths(&runtime_dir)
        .flat_map(|path| [path.clone(), path.join("lib")])
        .filter(|path| path.exists())
        .collect()
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

fn find_runtime_runner() -> Option<PathBuf> {
    if env::var_os("HEARTHSTONE_LINUX_DIRECT_LAUNCH").is_some() {
        return None;
    }
    if let Some(runner) = env::var_os("HEARTHSTONE_LINUX_RUNNER") {
        let runner = PathBuf::from(runner);
        if runner.exists() {
            return Some(runner);
        }
    }

    find_in_path("hearthstone-linux-gui-runtime")
}

fn ensure_bundled_interpreter(exe: &Path) -> Result<()> {
    let Some(interpreter) = bundled_interpreter() else {
        return Ok(());
    };
    if !interpreter.exists() {
        warn!(
            interpreter = %interpreter.display(),
            "bundled ELF interpreter was configured but does not exist"
        );
        return Ok(());
    }

    let Some(patchelf) = find_patchelf() else {
        warn!("bundled ELF interpreter was configured but patchelf was not found");
        return Ok(());
    };

    let current = Command::new(&patchelf)
        .arg("--print-interpreter")
        .arg(exe)
        .output()
        .with_context(|| {
            format!(
                "failed to inspect ELF interpreter with {}",
                patchelf.display()
            )
        })?;
    if current.status.success() {
        let current = String::from_utf8_lossy(&current.stdout).trim().to_string();
        if current == interpreter.to_string_lossy() {
            return Ok(());
        }
        if env::var_os("HEARTHSTONE_LINUX_FORCE_BUNDLED_INTERPRETER").is_none()
            && Path::new(&current).exists()
        {
            debug!(
                exe = %exe.display(),
                interpreter = %current,
                "Unity player ELF interpreter exists on this system"
            );
            return Ok(());
        }
    }

    info!(
        exe = %exe.display(),
        interpreter = %interpreter.display(),
        "patching Unity player ELF interpreter"
    );
    let status = Command::new(&patchelf)
        .arg("--set-interpreter")
        .arg(&interpreter)
        .arg(exe)
        .status()
        .with_context(|| format!("failed to run {}", patchelf.display()))?;
    anyhow::ensure!(
        status.success(),
        "{} failed to patch {}",
        patchelf.display(),
        exe.display()
    );
    Ok(())
}

fn bundled_interpreter() -> Option<PathBuf> {
    env::var_os("HEARTHSTONE_LINUX_ELF_INTERPRETER")
        .map(PathBuf::from)
        .or_else(|| {
            let runtime_dir = env::var_os("HEARTHSTONE_LINUX_RUNTIME_DIR")?;
            Some(PathBuf::from(runtime_dir).join("ld-linux-x86-64.so.2"))
        })
}

fn find_patchelf() -> Option<PathBuf> {
    env::var_os("HEARTHSTONE_LINUX_PATCHELF")
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .or_else(|| find_in_path("patchelf"))
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
