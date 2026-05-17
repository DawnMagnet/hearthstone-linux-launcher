use anyhow::Result;
use clap::Parser;

#[cfg(feature = "gui")]
mod ui;

#[derive(Debug, Parser)]
#[command(name = "hearthstone-linux-gui")]
#[command(about = "Native Linux manager for Hearthstone")]
struct Args {
    /// Battle.net browser callback URI.
    #[arg(long)]
    auth_callback: Option<String>,

    /// Start the game directly after validating local state.
    #[arg(long)]
    launch: bool,

    /// Install or update the managed game directory without opening the GUI.
    #[arg(long)]
    install: bool,

    /// Run without GTK; useful for CI smoke checks.
    #[arg(long)]
    no_gui: bool,

    /// Region to use with --install.
    #[arg(long)]
    region: Option<hearthstone_linux::Region>,

    /// Locale to use with --install.
    #[arg(long)]
    locale: Option<hearthstone_linux::Locale>,

    /// Encrypt a Battle.net login token into the managed game directory.
    #[arg(long)]
    write_token: Option<String>,

    /// URI passed by desktop x-scheme-handler invocations.
    uri: Option<String>,
}

fn main() -> Result<()> {
    let _log_guard = hearthstone_linux::logging::init();
    let args = Args::parse();
    let callback = args.auth_callback.or(args.uri);

    if let Some(uri) = callback {
        tracing::info!("handling auth callback");
        let paths = hearthstone_linux::paths::AppPaths::discover()?;
        hearthstone_linux::auth::handle_callback_uri(&paths, &uri)?;
        println!("Login token written for {:?}", paths.game_dir);
        return Ok(());
    }

    if let Some(token) = args.write_token {
        tracing::info!("writing token from command line");
        let paths = hearthstone_linux::paths::AppPaths::discover()?;
        let mut config = hearthstone_linux::AppConfig::load_or_default(&paths.config_file)?;
        let game_dir = config.game_dir.clone().unwrap_or(paths.game_dir);
        hearthstone_linux::auth::write_encrypted_token_for_current_user(
            &game_dir.join("token"),
            &token,
        )?;
        config.game_dir = Some(game_dir.clone());
        config.logged_in = true;
        config.save(&paths.config_file)?;
        println!("Login token written for {:?}", game_dir);
        return Ok(());
    }

    if args.install {
        tracing::info!(
            region = args.region.map(|region| region.as_str()),
            locale = args.locale.map(|locale| locale.as_str()),
            "starting command-line install"
        );
        let paths = hearthstone_linux::paths::AppPaths::discover()?;
        let mut config = hearthstone_linux::AppConfig::load_or_default(&paths.config_file)?;
        if let Some(region) = args.region {
            config.region = region;
        }
        if let Some(locale) = args.locale {
            config.locale = locale;
        }

        let manager = hearthstone_linux::install::manager::InstallManager::new(paths);
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(manager.install_or_update(&mut config, |event| match event {
            hearthstone_linux::install::manager::TaskEvent::Started(message) => {
                println!("{message}")
            }
            hearthstone_linux::install::manager::TaskEvent::Progress { message, fraction } => {
                if let Some(fraction) = fraction {
                    println!("{:>3.0}% {message}", fraction * 100.0);
                } else {
                    println!("{message}");
                }
            }
            hearthstone_linux::install::manager::TaskEvent::Finished(message) => {
                println!("{message}")
            }
            hearthstone_linux::install::manager::TaskEvent::Failed(message) => {
                eprintln!("Failed: {message}")
            }
            hearthstone_linux::install::manager::TaskEvent::Cancelled(message) => {
                eprintln!("{message}")
            }
        }))?;
        return Ok(());
    }

    if args.launch {
        tracing::info!("launching game from command line");
        let paths = hearthstone_linux::paths::AppPaths::discover()?;
        let config = hearthstone_linux::AppConfig::load_or_default(&paths.config_file)?;
        let game_dir = config.game_dir.unwrap_or(paths.game_dir);
        hearthstone_linux::install::launcher::launch_game(&game_dir)?;
        return Ok(());
    }

    if args.no_gui {
        tracing::info!("no-gui smoke check");
        println!("hearthstone-linux-gui core is available");
        return Ok(());
    }

    #[cfg(feature = "gui")]
    {
        tracing::info!("starting GUI");
        ui::run();
        Ok(())
    }

    #[cfg(not(feature = "gui"))]
    {
        anyhow::bail!("this binary was built without the `gui` feature")
    }
}
