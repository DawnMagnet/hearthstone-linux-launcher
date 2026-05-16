use gtk4 as gtk;
use libadwaita as adw;

use adw::prelude::*;
use gtk::{gio, glib};
use hearthstone_linux::{
    auth,
    config::{AppConfig, Locale, Region},
    install::{
        launcher,
        manager::{InstallManager, TaskEvent},
    },
    paths::AppPaths,
};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    process::Child,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
};

pub fn run() {
    tracing::debug!("creating GTK application");
    let app = adw::Application::builder()
        .application_id("io.github.hearthstone_linux")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    app.connect_startup(|_| configure_color_scheme());
    app.connect_open(handle_open);
    app.connect_activate(build_window);
    app.run();
}

fn handle_open(_app: &adw::Application, files: &[gio::File], _hint: &str) {
    let paths = match AppPaths::discover() {
        Ok(paths) => paths,
        Err(error) => {
            tracing::error!(error = %format!("{error:#}"), "failed to resolve paths for auth callback");
            return;
        }
    };

    for file in files {
        let uri = file.uri();
        tracing::info!(uri = %uri, "handling auth callback from open event");
        match auth::handle_callback_uri(&paths, uri.as_str()) {
            Ok(()) => tracing::info!("auth callback handled"),
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "failed to handle auth callback")
            }
        }
    }
}

fn configure_color_scheme() {
    let prefer_dark = gtk::Settings::default().is_some_and(|settings| {
        let prefer_dark = settings.is_gtk_application_prefer_dark_theme();
        if prefer_dark {
            settings.set_gtk_application_prefer_dark_theme(false);
        }
        prefer_dark
    });

    adw::StyleManager::default().set_color_scheme(if prefer_dark {
        adw::ColorScheme::PreferDark
    } else {
        adw::ColorScheme::Default
    });
}

fn build_window(app: &adw::Application) {
    tracing::debug!("building main window");
    let paths = Rc::new(AppPaths::discover().expect("XDG paths are required"));
    let config = Rc::new(RefCell::new(
        AppConfig::load_or_default(&paths.config_file).unwrap_or_default(),
    ));

    let title = adw::WindowTitle::new("Hearthstone Linux", "");
    let header = adw::HeaderBar::builder().title_widget(&title).build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.add_css_class("title-3");

    let version = gtk::Label::new(None);
    version.set_xalign(0.0);
    version.add_css_class("dim-label");

    let progress = gtk::ProgressBar::new();
    progress.set_show_text(true);
    progress.set_visible(false);

    let region = gtk::ComboBoxText::new();
    for item in Region::ALL {
        region.append(Some(item.as_str()), item.as_str());
    }
    region.set_active_id(Some(config.borrow().region.as_str()));

    let locale = gtk::ComboBoxText::new();
    for item in Locale::ALL {
        locale.append(Some(item.as_str()), item.as_str());
    }
    locale.set_active_id(Some(config.borrow().locale.as_str()));

    let install = gtk::Button::with_label("Install / Update");
    install.add_css_class("suggested-action");
    let login = gtk::Button::with_label("Login");
    let launch = gtk::Button::with_label("Play");
    launch.add_css_class("suggested-action");
    let refresh = gtk::Button::with_label("Refresh");

    let settings = gtk::Grid::new();
    settings.set_column_spacing(12);
    settings.set_row_spacing(8);
    settings.attach(&gtk::Label::new(Some("Region")), 0, 0, 1, 1);
    settings.attach(&region, 1, 0, 1, 1);
    settings.attach(&gtk::Label::new(Some("Locale")), 0, 1, 1, 1);
    settings.attach(&locale, 1, 1, 1, 1);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.append(&install);
    actions.append(&login);
    actions.append(&launch);
    actions.append(&refresh);

    content.append(&status);
    content.append(&version);
    content.append(&progress);
    content.append(&settings);
    content.append(&actions);
    root.append(&header);
    root.append(&content);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Hearthstone Linux")
        .default_width(620)
        .default_height(360)
        .content(&root)
        .build();

    update_status(&status, &version, &paths);
    update_login_button(&login, &paths);
    let login_waiting = Rc::new(Cell::new(false));
    let install_cancel = Rc::new(RefCell::new(None::<Arc<AtomicBool>>));
    let running_game = Rc::new(RefCell::new(None::<Child>));

    {
        let config = config.clone();
        region.connect_changed(move |combo| {
            if let Some(value) = combo.active_id() {
                if let Ok(parsed) = value.parse() {
                    config.borrow_mut().region = parsed;
                }
            }
        });
    }

    {
        let config = config.clone();
        locale.connect_changed(move |combo| {
            if let Some(value) = combo.active_id() {
                if let Ok(parsed) = value.parse() {
                    config.borrow_mut().locale = parsed;
                }
            }
        });
    }

    {
        let paths = paths.clone();
        let config = config.clone();
        let status = status.clone();
        let version = version.clone();
        let progress = progress.clone();
        let install_button = install.clone();
        let install_cancel = install_cancel.clone();
        install.connect_clicked(move |_| {
            if let Some(cancel) = install_cancel.borrow().as_ref() {
                tracing::info!("install stop requested from UI");
                cancel.store(true, Ordering::Relaxed);
                set_install_stopping(&install_button);
                status.set_text("Stopping installation");
                return;
            }

            let install_action = if paths.game_dir.join("Bin/Hearthstone.x86_64").exists() {
                "Update"
            } else {
                "Install"
            };
            let cancel = Arc::new(AtomicBool::new(false));
            *install_cancel.borrow_mut() = Some(cancel.clone());
            set_install_running(&install_button, install_action);
            tracing::info!(action = install_action, "install/update requested from UI");
            progress.set_visible(true);
            progress.set_fraction(0.0);
            progress.set_text(Some("0%"));
            status.set_text("Preparing");

            let (sender, receiver) = mpsc::channel::<TaskEvent>();
            let paths_for_thread = (*paths).clone();
            let mut config_for_thread = config.borrow().clone();
            let cancel_for_thread = cancel.clone();
            std::thread::spawn(move || {
                let manager = InstallManager::new(paths_for_thread);
                let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
                let result = runtime.block_on(manager.install_or_update_cancellable(
                    &mut config_for_thread,
                    |event| {
                        let _ = sender.send(event);
                    },
                    cancel_for_thread.clone(),
                ));
                if let Err(error) = result {
                    tracing::error!(error = %format!("{error:#}"), "install/update failed");
                    let event = if cancel_for_thread.load(Ordering::Relaxed) {
                        TaskEvent::Cancelled("Installation cancelled".into())
                    } else {
                        TaskEvent::Failed(format!("{error:#}"))
                    };
                    let _ = sender.send(event);
                }
            });

            let paths = paths.clone();
            let config = config.clone();
            let install_button = install_button.clone();
            let install_cancel = install_cancel.clone();
            let status = status.clone();
            let version = version.clone();
            let progress = progress.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
                while let Ok(event) = receiver.try_recv() {
                    match event {
                        TaskEvent::Started(message) => {
                            status.set_text(&message);
                            progress.pulse();
                        }
                        TaskEvent::Progress { message, fraction } => {
                            status.set_text(&message);
                            if let Some(fraction) = fraction {
                                progress.set_fraction(fraction);
                                progress.set_text(Some(&format!("{:.0}%", fraction * 100.0)));
                            } else {
                                progress.pulse();
                                progress.set_text(None);
                            }
                        }
                        TaskEvent::Finished(message) => {
                            tracing::info!("install/update finished");
                            status.set_text(&message);
                            progress.set_fraction(1.0);
                            progress.set_text(Some("100%"));
                            progress.set_visible(false);
                            *install_cancel.borrow_mut() = None;
                            set_install_idle(&install_button);
                            sync_config_from_disk(&paths, &config);
                            update_status(&status, &version, &paths);
                            return glib::ControlFlow::Break;
                        }
                        TaskEvent::Failed(message) => {
                            tracing::error!(error = %message, "install/update failed in UI");
                            status.set_text(&format!("Failed: {message}"));
                            progress.set_visible(false);
                            *install_cancel.borrow_mut() = None;
                            set_install_idle(&install_button);
                            return glib::ControlFlow::Break;
                        }
                        TaskEvent::Cancelled(message) => {
                            tracing::info!("install/update cancelled");
                            status.set_text(&message);
                            progress.set_visible(false);
                            *install_cancel.borrow_mut() = None;
                            set_install_idle(&install_button);
                            sync_config_from_disk(&paths, &config);
                            update_status(&status, &version, &paths);
                            return glib::ControlFlow::Break;
                        }
                    }
                }
                glib::ControlFlow::Continue
            });
        });
    }

    {
        let paths = paths.clone();
        let config = config.clone();
        let status = status.clone();
        let version = version.clone();
        let login_button = login.clone();
        let login_waiting = login_waiting.clone();
        login.connect_clicked(move |_| {
            if login_waiting.get() {
                tracing::info!("login wait cancelled from UI");
                login_waiting.set(false);
                set_login_idle(&login_button, &paths);
                status.set_text("Login cancelled");
                return;
            }

            if paths.game_token().exists() {
                tracing::debug!("login token already exists");
                mark_logged_in(&paths, &config);
                sync_config_from_disk(&paths, &config);
                status.set_text("Already logged in");
                update_status(&status, &version, &paths);
                update_login_button(&login_button, &paths);
                return;
            }

            let mut current = config.borrow().clone();
            preserve_install_metadata(&paths, &mut current);
            current.game_dir = Some(paths.game_dir.clone());
            let login_url = current.region.login_url();
            let _ = current.save(&paths.config_file);
            if let Err(error) = ensure_auth_scheme_handlers() {
                tracing::warn!(error = %format!("{error:#}"), "failed to register auth URI handlers");
            }

            login_waiting.set(true);
            set_login_waiting(&login_button);
            status.set_text("Waiting for browser login");
            tracing::info!(region = %current.region, "opening browser login");

            let _ = gio::AppInfo::launch_default_for_uri(login_url, None::<&gio::AppLaunchContext>);

            let paths = paths.clone();
            let config = config.clone();
            let status = status.clone();
            let version = version.clone();
            let login_button = login_button.clone();
            let login_waiting = login_waiting.clone();
            glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
                if !login_waiting.get() {
                    return glib::ControlFlow::Break;
                }

                if paths.game_token().exists() {
                    tracing::info!("browser login completed");
                    login_waiting.set(false);
                    mark_logged_in(&paths, &config);
                    sync_config_from_disk(&paths, &config);
                    status.set_text("Login complete");
                    update_status(&status, &version, &paths);
                    update_login_button(&login_button, &paths);
                    return glib::ControlFlow::Break;
                }

                glib::ControlFlow::Continue
            });
        });
    }

    {
        let paths = paths.clone();
        let config = config.clone();
        let status = status.clone();
        let launch_button = launch.clone();
        let running_game = running_game.clone();
        launch.connect_clicked(move |_| {
            if let Some(child) = running_game.borrow_mut().as_mut() {
                tracing::info!("game stop requested from UI");
                match child.kill() {
                    Ok(()) => {
                        set_launch_stopping(&launch_button);
                        status.set_text("Stopping game");
                    }
                    Err(error) => {
                        tracing::error!(error = %error, "failed to stop game");
                        status.set_text(&format!("Failed to stop game: {error}"));
                    }
                }
                return;
            }

            let game_dir = config
                .borrow()
                .game_dir
                .clone()
                .unwrap_or(paths.game_dir.clone());
            tracing::info!(game_dir = %game_dir.display(), "launch requested from UI");
            match launcher::launch_game(&game_dir) {
                Ok(child) => {
                    status.set_text("Game running");
                    set_launch_running(&launch_button);
                    *running_game.borrow_mut() = Some(child);

                    let status = status.clone();
                    let launch_button = launch_button.clone();
                    let running_game = running_game.clone();
                    glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
                        let mut game = running_game.borrow_mut();
                        let Some(child) = game.as_mut() else {
                            set_launch_idle(&launch_button);
                            return glib::ControlFlow::Break;
                        };

                        match child.try_wait() {
                            Ok(Some(exit)) => {
                                tracing::info!(status = %exit, "game exited");
                                status.set_text("Game stopped");
                                *game = None;
                                set_launch_idle(&launch_button);
                                glib::ControlFlow::Break
                            }
                            Ok(None) => glib::ControlFlow::Continue,
                            Err(error) => {
                                tracing::error!(error = %error, "failed to poll game process");
                                status.set_text(&format!("Game status error: {error}"));
                                *game = None;
                                set_launch_idle(&launch_button);
                                glib::ControlFlow::Break
                            }
                        }
                    });
                }
                Err(error) => {
                    tracing::error!(error = %format!("{error:#}"), "launch failed");
                    status.set_text(&format!("Launch failed: {error:#}"));
                }
            }
        });
    }

    {
        let paths = paths.clone();
        let status = status.clone();
        let version = version.clone();
        let login = login.clone();
        let config = config.clone();
        refresh.connect_clicked(move |_| {
            tracing::debug!("refresh requested from UI");
            sync_config_from_disk(&paths, &config);
            update_status(&status, &version, &paths);
            update_login_button(&login, &paths);
        });
    }

    window.present();
}

fn update_status(status: &gtk::Label, version: &gtk::Label, paths: &AppPaths) {
    let installed = paths.game_dir.join("Bin/Hearthstone.x86_64").exists();
    let token = paths.game_token().exists();
    match (installed, token) {
        (true, true) => status.set_text("Ready"),
        (true, false) => status.set_text("Login Required"),
        (false, _) => status.set_text("Not Installed"),
    }

    let config = reconcile_status_config(paths, token);
    let login = if config.logged_in && token {
        "Logged in"
    } else if token {
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
    version.set_text(&format!(
        "Login: {login} · Game: {game} · Unity: {unity} · {} / {}",
        config.region, config.locale
    ));
}

fn update_login_button(button: &gtk::Button, paths: &AppPaths) {
    if paths.game_token().exists() {
        button.set_label("Logged In");
        button.remove_css_class("destructive-action");
        button.add_css_class("suggested-action");
    } else {
        set_login_idle(button, paths);
    }
}

fn set_install_idle(button: &gtk::Button) {
    button.set_sensitive(true);
    button.set_label("Install / Update");
    button.remove_css_class("destructive-action");
    button.add_css_class("suggested-action");
}

fn set_install_running(button: &gtk::Button, action: &str) {
    button.set_sensitive(true);
    button.set_label(&format!("Stop {action}"));
    button.remove_css_class("suggested-action");
    button.add_css_class("destructive-action");
}

fn set_install_stopping(button: &gtk::Button) {
    button.set_label("Stopping...");
    button.remove_css_class("suggested-action");
    button.add_css_class("destructive-action");
    button.set_sensitive(false);
}

fn set_login_idle(button: &gtk::Button, _paths: &AppPaths) {
    button.set_label("Login");
    button.remove_css_class("destructive-action");
    button.remove_css_class("suggested-action");
}

fn set_login_waiting(button: &gtk::Button) {
    button.set_label("Stop");
    button.remove_css_class("suggested-action");
    button.add_css_class("destructive-action");
}

fn set_launch_idle(button: &gtk::Button) {
    button.set_sensitive(true);
    button.set_label("Play");
    button.remove_css_class("destructive-action");
    button.add_css_class("suggested-action");
}

fn set_launch_running(button: &gtk::Button) {
    button.set_sensitive(true);
    button.set_label("Stop");
    button.remove_css_class("suggested-action");
    button.add_css_class("destructive-action");
}

fn set_launch_stopping(button: &gtk::Button) {
    button.set_label("Stopping...");
    button.remove_css_class("suggested-action");
    button.add_css_class("destructive-action");
    button.set_sensitive(false);
}

fn mark_logged_in(paths: &AppPaths, config: &Rc<RefCell<AppConfig>>) {
    let mut current = config.borrow_mut();
    preserve_install_metadata(paths, &mut current);
    current.game_dir = Some(paths.game_dir.clone());
    current.logged_in = true;
    current.last_login_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs().to_string());
    let _ = current.save(&paths.config_file);
}

fn sync_config_from_disk(paths: &AppPaths, config: &Rc<RefCell<AppConfig>>) {
    if let Ok(saved) = AppConfig::load_or_default(&paths.config_file) {
        *config.borrow_mut() = saved;
    }
}

fn preserve_install_metadata(paths: &AppPaths, config: &mut AppConfig) {
    let Ok(saved) = AppConfig::load_or_default(&paths.config_file) else {
        return;
    };
    if saved.installed_version.is_some() {
        config.installed_version = saved.installed_version;
    }
    if saved.unity_version.is_some() {
        config.unity_version = saved.unity_version;
    }
}

fn reconcile_status_config(paths: &AppPaths, token_exists: bool) -> AppConfig {
    let mut config = AppConfig::load_or_default(&paths.config_file).unwrap_or_default();
    let should_save = config.logged_in != token_exists || config.game_dir.is_none();
    config.logged_in = token_exists;
    if config.game_dir.is_none() {
        config.game_dir = Some(paths.game_dir.clone());
    }
    if should_save {
        let _ = config.save(&paths.config_file);
    }
    config
}

fn ensure_auth_scheme_handlers() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let Some(applications_dir) = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .map(|home| home.join(".local/share"))
        })
        .map(|data_home| data_home.join("applications"))
    else {
        return Ok(());
    };
    std::fs::create_dir_all(&applications_dir)?;

    let desktop_id = "io.github.hearthstone_linux.desktop";
    let desktop_file = applications_dir.join(desktop_id);
    std::fs::write(&desktop_file, user_desktop_entry(&exe))?;

    let _ = std::process::Command::new("update-desktop-database")
        .arg(&applications_dir)
        .status();
    for mime in [
        "x-scheme-handler/wtcg",
        "x-scheme-handler/blizzard-hearthstone",
        "x-scheme-handler/hearthstone-linux",
    ] {
        let _ = std::process::Command::new("xdg-mime")
            .args(["default", desktop_id, mime])
            .status();
    }

    Ok(())
}

fn user_desktop_entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Hearthstone Linux\nExec={} %u\nIcon=io.github.hearthstone_linux\nCategories=Game;\nMimeType=x-scheme-handler/wtcg;x-scheme-handler/blizzard-hearthstone;x-scheme-handler/hearthstone-linux;\nStartupNotify=true\n",
        shell_quote_path(exe)
    )
}

fn shell_quote_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
