mod app;
mod auth;
mod browser;
mod status;

use relm4::gtk::prelude::*;
use relm4::{
    adw,
    gtk::{
        self,
        gio::{self, prelude::ApplicationExtManual},
    },
    RelmApp,
};

use hearthstone_linux::{auth as core_auth, paths::AppPaths};

pub fn run() {
    tracing::debug!("creating Relm4 application");
    let gtk_app = adw::Application::builder()
        .application_id("io.github.hearthstone_linux_gui")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    gtk_app.connect_startup(|_| configure_color_scheme());
    gtk_app.connect_open(handle_open);

    let relm_app = RelmApp::from_app(gtk_app);
    relm_app.run::<app::MainWindow>(app::AppInit::load());
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
        match core_auth::handle_callback_uri(&paths, uri.as_str()) {
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
