use hearthstone_linux::{config::AppConfig, paths::AppPaths};

#[derive(Clone, Debug)]
pub struct StatusSnapshot {
    pub headline: String,
    pub details: String,
}

pub fn reconcile(paths: &AppPaths) -> (AppConfig, StatusSnapshot) {
    let token_exists = paths.game_token().exists();
    let mut config = AppConfig::load_or_default(&paths.config_file).unwrap_or_default();
    let should_save = config.logged_in != token_exists || config.game_dir.is_none();
    config.logged_in = token_exists;
    if config.game_dir.is_none() {
        config.game_dir = Some(paths.game_dir.clone());
    }
    if should_save {
        let _ = config.save(&paths.config_file);
    }

    let installed = paths.game_dir.join("Bin/Hearthstone.x86_64").exists();
    let headline = match (installed, token_exists) {
        (true, true) => "Ready",
        (true, false) => "Login Required",
        (false, _) => "Not Installed",
    }
    .to_string();

    (config.clone(), snapshot(headline, &config, token_exists))
}

pub fn snapshot(
    headline: impl Into<String>,
    config: &AppConfig,
    token_exists: bool,
) -> StatusSnapshot {
    let login = if config.logged_in && token_exists {
        "Logged in"
    } else if token_exists {
        "Token present"
    } else {
        "Logged out"
    };
    let game = config
        .installed_version
        .as_deref()
        .filter(|version| !version.is_empty())
        .unwrap_or("Not installed");
    let unity = config
        .unity_version
        .as_deref()
        .filter(|version| !version.is_empty())
        .unwrap_or("Not installed");

    StatusSnapshot {
        headline: headline.into(),
        details: format!(
            "Login: {login} · Game: {game} · Unity: {unity} · {} / {}",
            config.region, config.locale
        ),
    }
}
