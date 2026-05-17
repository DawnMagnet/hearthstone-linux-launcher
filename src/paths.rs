use anyhow::{Context, Result};
use directories::ProjectDirs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
    pub config_file: PathBuf,
    pub game_dir: PathBuf,
    pub ngdp_dir: PathBuf,
    pub unity_cache_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from(
            "io.github",
            "hearthstone-linux-gui",
            "hearthstone-linux-gui",
        )
        .context("could not resolve XDG project directories")?;
        let config_dir = dirs.config_dir().to_path_buf();
        let data_dir = dirs.data_dir().to_path_buf();
        let cache_dir = dirs.cache_dir().to_path_buf();
        let state_dir = dirs
            .state_dir()
            .unwrap_or_else(|| dirs.data_dir())
            .to_path_buf();

        Ok(Self {
            config_file: config_dir.join("config.toml"),
            game_dir: data_dir.join("game"),
            ngdp_dir: cache_dir.join("ngdp"),
            unity_cache_dir: cache_dir.join("unity"),
            log_dir: state_dir.join("logs"),
            config_dir,
            data_dir,
            cache_dir,
            state_dir,
        })
    }

    pub fn ensure(&self) -> Result<()> {
        for dir in [
            &self.config_dir,
            &self.data_dir,
            &self.cache_dir,
            &self.state_dir,
            &self.game_dir,
            &self.ngdp_dir,
            &self.unity_cache_dir,
            &self.log_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn game_token(&self) -> PathBuf {
        self.game_dir.join("token")
    }
}
