use super::{auth, browser, status};
use hearthstone_linux::{
    auth::{start_local_callback_server, LocalCallbackServer},
    config::{AppConfig, Locale, Region},
    install::{
        launcher,
        manager::{InstallManager, TaskEvent},
    },
    paths::AppPaths,
};
use relm4::adw::prelude::*;
use relm4::{adw, gtk, gtk::glib, Component, ComponentParts, ComponentSender};
use std::{
    process::Child,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

pub struct AppInit {
    paths: AppPaths,
    config: AppConfig,
    snapshot: status::StatusSnapshot,
}

impl AppInit {
    pub fn load() -> Self {
        let paths = AppPaths::discover().expect("XDG paths are required");
        let (config, snapshot) = status::reconcile(&paths);
        Self {
            paths,
            config,
            snapshot,
        }
    }
}

pub struct MainWindow {
    paths: AppPaths,
    config: AppConfig,
    status: status::StatusSnapshot,
    install_state: InstallState,
    install_cancel: Option<Arc<AtomicBool>>,
    login_session: Option<LoginSession>,
    game_session: Option<GameSession>,
    progress: ProgressState,
}

#[derive(Debug)]
pub enum AppMsg {
    RegionChanged(String),
    LocaleChanged(String),
    InstallPressed,
    InstallEvent(TaskEvent),
    LoginPressed,
    Logout,
    SwitchAccount,
    LoginPoll,
    LaunchPressed,
    GamePoll,
    Refresh,
}

struct LoginSession {
    cancel: Arc<AtomicBool>,
    callback: Rc<LocalCallbackServer>,
}

struct GameSession {
    child: Child,
    poll_cancel: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InstallState {
    Idle,
    Running { action: String },
    Stopping,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LoginState {
    Idle,
    Waiting,
    LoggedIn,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum LaunchState {
    Idle,
    Running,
    Stopping,
}

#[derive(Clone, Debug, Default)]
struct ProgressState {
    visible: bool,
    fraction: Option<f64>,
    text: Option<String>,
}

pub struct MainWidgets {
    status: gtk::Label,
    details: gtk::Label,
    progress: gtk::ProgressBar,
    region: gtk::ComboBoxText,
    locale: gtk::ComboBoxText,
    install: gtk::Button,
    login: gtk::Button,
    launch: gtk::Button,
}

impl Component for MainWindow {
    type CommandOutput = ();
    type Init = AppInit;
    type Input = AppMsg;
    type Output = ();
    type Root = adw::ApplicationWindow;
    type Widgets = MainWidgets;

    fn init_root() -> Self::Root {
        adw::ApplicationWindow::builder()
            .title("hearthstone-linux-gui")
            .default_width(620)
            .default_height(360)
            .build()
    }

    fn init(
        init: Self::Init,
        window: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        tracing::debug!("building Relm4 main window");
        let model = MainWindow {
            paths: init.paths,
            config: init.config,
            status: init.snapshot,
            install_state: InstallState::Idle,
            install_cancel: None,
            login_session: None,
            game_session: None,
            progress: ProgressState::default(),
        };

        let mut widgets = build_widgets(&window, &sender);
        model.update_view(&mut widgets, sender);
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            AppMsg::RegionChanged(value) => {
                if let Ok(region) = value.parse() {
                    self.config.region = region;
                    self.refresh_details();
                }
            }
            AppMsg::LocaleChanged(value) => {
                if let Ok(locale) = value.parse() {
                    self.config.locale = locale;
                    self.refresh_details();
                }
            }
            AppMsg::InstallPressed => self.handle_install_pressed(sender.clone()),
            AppMsg::InstallEvent(event) => self.handle_install_event(event),
            AppMsg::LoginPressed => self.handle_login_pressed(widgets, sender.clone()),
            AppMsg::Logout => self.handle_logout(),
            AppMsg::SwitchAccount => self.handle_switch_account(sender.clone()),
            AppMsg::LoginPoll => self.handle_login_poll(),
            AppMsg::LaunchPressed => self.handle_launch_pressed(sender.clone()),
            AppMsg::GamePoll => self.handle_game_poll(),
            AppMsg::Refresh => self.refresh_from_disk(),
        }

        self.update_view(widgets, sender);
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.status.set_text(&self.status.headline);
        widgets.details.set_text(&self.status.details);

        if widgets.region.active_id().as_deref() != Some(self.config.region.as_str()) {
            widgets
                .region
                .set_active_id(Some(self.config.region.as_str()));
        }
        if widgets.locale.active_id().as_deref() != Some(self.config.locale.as_str()) {
            widgets
                .locale
                .set_active_id(Some(self.config.locale.as_str()));
        }

        widgets.progress.set_visible(self.progress.visible);
        match self.progress.fraction {
            Some(fraction) => widgets.progress.set_fraction(fraction),
            None if self.progress.visible => widgets.progress.pulse(),
            None => widgets.progress.set_fraction(0.0),
        }
        widgets.progress.set_text(self.progress.text.as_deref());

        apply_install_state(&widgets.install, &self.install_state);
        apply_login_state(&widgets.login, self.login_state());
        apply_launch_state(&widgets.launch, self.launch_state());
    }
}

impl MainWindow {
    fn handle_install_pressed(&mut self, sender: ComponentSender<Self>) {
        if let InstallState::Running { .. } = self.install_state {
            self.stop_install();
            return;
        }
        if self.install_state == InstallState::Stopping {
            return;
        }

        let action = if self.paths.game_dir.join("Bin/Hearthstone.x86_64").exists() {
            "Update"
        } else {
            "Install"
        }
        .to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        self.install_state = InstallState::Running {
            action: action.clone(),
        };
        self.progress = ProgressState {
            visible: true,
            fraction: Some(0.0),
            text: Some("0%".into()),
        };
        self.status.headline = "Preparing".into();

        tracing::info!(action = action, "install/update requested from UI");
        let paths = self.paths.clone();
        let mut config = self.config.clone();
        let cancel_for_thread = cancel.clone();
        let input = sender.input_sender().clone();
        std::thread::spawn(move || {
            let manager = InstallManager::new(paths);
            let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
            let result = runtime.block_on(manager.install_or_update_cancellable(
                &mut config,
                |event| input.emit(AppMsg::InstallEvent(event)),
                cancel_for_thread.clone(),
            ));
            if let Err(error) = result {
                tracing::error!(error = %format!("{error:#}"), "install/update failed");
                let event = if cancel_for_thread.load(Ordering::Relaxed) {
                    TaskEvent::Cancelled("Installation cancelled".into())
                } else {
                    TaskEvent::Failed(format!("{error:#}"))
                };
                input.emit(AppMsg::InstallEvent(event));
            }
        });

        self.install_cancel = Some(cancel);
    }

    fn stop_install(&mut self) {
        tracing::info!("install stop requested from UI");
        if let Some(cancel) = self.install_cancel.as_ref() {
            cancel.store(true, Ordering::Relaxed);
            self.install_state = InstallState::Stopping;
            self.status.headline = "Stopping installation".into();
        }
    }

    fn handle_install_event(&mut self, event: TaskEvent) {
        match event {
            TaskEvent::Started(message) => {
                self.status.headline = message;
                self.progress.visible = true;
                self.progress.fraction = None;
                self.progress.text = None;
            }
            TaskEvent::Progress { message, fraction } => {
                self.status.headline = message;
                self.progress.visible = true;
                self.progress.fraction = fraction;
                self.progress.text = fraction.map(|value| format!("{:.0}%", value * 100.0));
            }
            TaskEvent::Finished(message) => {
                tracing::info!("install/update finished");
                self.install_state = InstallState::Idle;
                self.install_cancel = None;
                self.progress = ProgressState {
                    visible: false,
                    fraction: Some(1.0),
                    text: Some("100%".into()),
                };
                auth::sync_config_from_disk(&self.paths, &mut self.config);
                self.refresh_status_with_headline(message);
            }
            TaskEvent::Failed(message) => {
                tracing::error!(error = %message, "install/update failed in UI");
                self.install_state = InstallState::Idle;
                self.install_cancel = None;
                self.progress.visible = false;
                self.status.headline = format!("Failed: {message}");
                self.refresh_details();
            }
            TaskEvent::Cancelled(message) => {
                tracing::info!("install/update cancelled");
                self.install_state = InstallState::Idle;
                self.install_cancel = None;
                self.progress.visible = false;
                auth::sync_config_from_disk(&self.paths, &mut self.config);
                self.refresh_status_with_headline(message);
            }
        }
    }

    fn handle_login_pressed(&mut self, widgets: &MainWidgets, sender: ComponentSender<Self>) {
        if let Some(session) = self.login_session.take() {
            tracing::info!("login wait cancelled from UI");
            session.cancel.store(true, Ordering::Relaxed);
            session.callback.cancel.store(true, Ordering::Relaxed);
            self.status.headline = "Login cancelled".into();
            self.refresh_details();
            return;
        }

        if self.paths.game_token().exists() {
            tracing::debug!("login token already exists");
            show_account_popover(&widgets.login, sender);
            return;
        }

        self.begin_login(sender);
    }

    fn handle_logout(&mut self) {
        match auth::mark_logged_out(&self.paths, &mut self.config) {
            Ok(()) => {
                auth::sync_config_from_disk(&self.paths, &mut self.config);
                self.refresh_status_with_headline("Logged out");
            }
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "logout failed");
                self.status.headline = format!("Logout failed: {error:#}");
                self.refresh_details();
            }
        }
    }

    fn handle_switch_account(&mut self, sender: ComponentSender<Self>) {
        match auth::mark_logged_out(&self.paths, &mut self.config) {
            Ok(()) => self.begin_login(sender),
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "failed to clear previous login");
                self.status.headline = format!("Switch account failed: {error:#}");
                self.refresh_details();
            }
        }
    }

    fn begin_login(&mut self, sender: ComponentSender<Self>) {
        if let Some(session) = self.login_session.take() {
            session.cancel.store(true, Ordering::Relaxed);
            session.callback.cancel.store(true, Ordering::Relaxed);
        }

        let mut current = self.config.clone();
        auth::preserve_install_metadata(&self.paths, &mut current);
        current.game_dir = Some(self.paths.game_dir.clone());
        if let Err(error) = current.save(&self.paths.config_file) {
            tracing::error!(error = %format!("{error:#}"), "failed to save login settings");
            self.status.headline = format!("Login setup failed: {error:#}");
            self.refresh_details();
            return;
        }

        let callback = match start_local_callback_server(self.paths.clone(), current.region) {
            Ok(callback) => Rc::new(callback),
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "failed to start auth callback server");
                self.status.headline = format!("Login setup failed: {error:#}");
                self.refresh_details();
                return;
            }
        };

        let cancel = Arc::new(AtomicBool::new(false));
        self.login_session = Some(LoginSession {
            cancel: cancel.clone(),
            callback,
        });

        if let Err(error) = auth::ensure_auth_scheme_handlers() {
            tracing::warn!(error = %format!("{error:#}"), "failed to register auth URI handlers");
            self.status.headline =
                "Login handler registration failed; continuing with browser login".into();
        } else {
            self.status.headline = "Complete login in browser; waiting for desktop callback".into();
        }
        self.refresh_details();

        let login_url = current.region.login_url().to_string();
        tracing::info!(region = %current.region, "opening browser login with desktop callback handler");
        if let Err(error) = browser::open_login_url(&login_url) {
            tracing::error!(
                url = login_url,
                error = %format!("{error:#}"),
                "failed to open browser login"
            );
            self.status.headline = format!("Could not open browser. URL: {login_url}");
        }

        let input = sender.input_sender().clone();
        glib::timeout_add_local(Duration::from_secs(1), move || {
            if cancel.load(Ordering::Relaxed) {
                glib::ControlFlow::Break
            } else {
                input.emit(AppMsg::LoginPoll);
                glib::ControlFlow::Continue
            }
        });
    }

    fn handle_login_poll(&mut self) {
        let Some(session) = self.login_session.as_ref() else {
            return;
        };
        if session.cancel.load(Ordering::Relaxed) {
            return;
        }

        let token_exists = self.paths.game_token().exists();
        let config_logged_in = AppConfig::load_or_default(&self.paths.config_file)
            .map(|config| config.logged_in)
            .unwrap_or(false);
        if token_exists || config_logged_in {
            tracing::info!("browser login completed");
            if let Some(session) = self.login_session.take() {
                session.cancel.store(true, Ordering::Relaxed);
            }
            auth::sync_config_from_disk(&self.paths, &mut self.config);
            self.refresh_status_with_headline("Login complete");
        }
    }

    fn handle_launch_pressed(&mut self, sender: ComponentSender<Self>) {
        if let Some(session) = self.game_session.as_mut() {
            tracing::info!("game stop requested from UI");
            match session.child.kill() {
                Ok(()) => {
                    self.status.headline = "Stopping game".into();
                    self.refresh_details();
                }
                Err(error) => {
                    tracing::error!(error = %error, "failed to stop game");
                    self.status.headline = format!("Failed to stop game: {error}");
                    self.refresh_details();
                }
            }
            return;
        }

        let game_dir = self
            .config
            .game_dir
            .clone()
            .unwrap_or_else(|| self.paths.game_dir.clone());
        tracing::info!(game_dir = %game_dir.display(), "launch requested from UI");
        match launcher::launch_game(&game_dir) {
            Ok(child) => {
                let poll_cancel = Arc::new(AtomicBool::new(false));
                self.game_session = Some(GameSession {
                    child,
                    poll_cancel: poll_cancel.clone(),
                });
                self.status.headline = "Game running".into();
                self.refresh_details();

                let input = sender.input_sender().clone();
                glib::timeout_add_local(Duration::from_secs(1), move || {
                    if poll_cancel.load(Ordering::Relaxed) {
                        glib::ControlFlow::Break
                    } else {
                        input.emit(AppMsg::GamePoll);
                        glib::ControlFlow::Continue
                    }
                });
            }
            Err(error) => {
                tracing::error!(error = %format!("{error:#}"), "launch failed");
                self.status.headline = format!("Launch failed: {error:#}");
                self.refresh_details();
            }
        }
    }

    fn handle_game_poll(&mut self) {
        let Some(session) = self.game_session.as_mut() else {
            return;
        };

        match session.child.try_wait() {
            Ok(Some(exit)) => {
                tracing::info!(status = %exit, "game exited");
                session.poll_cancel.store(true, Ordering::Relaxed);
                self.game_session = None;
                self.status.headline = "Game stopped".into();
                self.refresh_details();
            }
            Ok(None) => {}
            Err(error) => {
                tracing::error!(error = %error, "failed to poll game process");
                session.poll_cancel.store(true, Ordering::Relaxed);
                self.game_session = None;
                self.status.headline = format!("Game status error: {error}");
                self.refresh_details();
            }
        }
    }

    fn refresh_from_disk(&mut self) {
        tracing::debug!("refresh requested from UI");
        let (config, snapshot) = status::reconcile(&self.paths);
        self.config = config;
        self.status = snapshot;
    }

    fn refresh_status_with_headline(&mut self, headline: impl Into<String>) {
        self.status = status::snapshot(headline, &self.config, self.paths.game_token().exists());
    }

    fn refresh_details(&mut self) {
        let headline = self.status.headline.clone();
        self.refresh_status_with_headline(headline);
    }

    fn login_state(&self) -> LoginState {
        if self.login_session.is_some() {
            LoginState::Waiting
        } else if self.paths.game_token().exists() {
            LoginState::LoggedIn
        } else {
            LoginState::Idle
        }
    }

    fn launch_state(&self) -> LaunchState {
        match self.status.headline.as_str() {
            "Stopping game" if self.game_session.is_some() => LaunchState::Stopping,
            _ if self.game_session.is_some() => LaunchState::Running,
            _ => LaunchState::Idle,
        }
    }
}

fn build_widgets(
    window: &adw::ApplicationWindow,
    sender: &ComponentSender<MainWindow>,
) -> MainWidgets {
    let title = adw::WindowTitle::new("hearthstone-linux-gui", "");
    let header = adw::HeaderBar::builder().title_widget(&title).build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.set_selectable(true);
    status.add_css_class("title-3");

    let details = gtk::Label::new(None);
    details.set_xalign(0.0);
    details.add_css_class("dim-label");

    let progress = gtk::ProgressBar::new();
    progress.set_show_text(true);
    progress.set_visible(false);

    let region = gtk::ComboBoxText::new();
    for item in Region::ALL {
        region.append(Some(item.as_str()), item.as_str());
    }
    {
        let input = sender.input_sender().clone();
        region.connect_changed(move |combo| {
            if let Some(value) = combo.active_id() {
                input.emit(AppMsg::RegionChanged(value.to_string()));
            }
        });
    }

    let locale = gtk::ComboBoxText::new();
    for item in Locale::ALL {
        locale.append(Some(item.as_str()), item.as_str());
    }
    {
        let input = sender.input_sender().clone();
        locale.connect_changed(move |combo| {
            if let Some(value) = combo.active_id() {
                input.emit(AppMsg::LocaleChanged(value.to_string()));
            }
        });
    }

    let install = gtk::Button::with_label("Install / Update");
    install.add_css_class("suggested-action");
    {
        let input = sender.input_sender().clone();
        install.connect_clicked(move |_| input.emit(AppMsg::InstallPressed));
    }

    let login = gtk::Button::with_label("Login");
    {
        let input = sender.input_sender().clone();
        login.connect_clicked(move |_| input.emit(AppMsg::LoginPressed));
    }

    let launch = gtk::Button::with_label("Play");
    launch.add_css_class("suggested-action");
    {
        let input = sender.input_sender().clone();
        launch.connect_clicked(move |_| input.emit(AppMsg::LaunchPressed));
    }

    let refresh = gtk::Button::with_label("Refresh");
    {
        let input = sender.input_sender().clone();
        refresh.connect_clicked(move |_| input.emit(AppMsg::Refresh));
    }

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
    content.append(&details);
    content.append(&progress);
    content.append(&settings);
    content.append(&actions);
    root.append(&header);
    root.append(&content);
    window.set_content(Some(&root));

    MainWidgets {
        status,
        details,
        progress,
        region,
        locale,
        install,
        login,
        launch,
    }
}

fn show_account_popover(anchor: &gtk::Button, sender: ComponentSender<MainWindow>) {
    let popover = gtk::Popover::new();
    popover.set_parent(anchor);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);

    let switch_account = gtk::Button::with_label("Switch Account");
    let logout = gtk::Button::with_label("Log Out");
    logout.add_css_class("destructive-action");

    {
        let popover = popover.clone();
        let input = sender.input_sender().clone();
        switch_account.connect_clicked(move |_| {
            popover.popdown();
            input.emit(AppMsg::SwitchAccount);
        });
    }
    {
        let popover = popover.clone();
        let input = sender.input_sender().clone();
        logout.connect_clicked(move |_| {
            popover.popdown();
            input.emit(AppMsg::Logout);
        });
    }

    content.append(&switch_account);
    content.append(&logout);
    popover.set_child(Some(&content));
    popover.connect_closed(|popover| popover.unparent());
    popover.popup();
}

fn apply_install_state(button: &gtk::Button, state: &InstallState) {
    match state {
        InstallState::Idle => {
            button.set_sensitive(true);
            button.set_label("Install / Update");
            button.remove_css_class("destructive-action");
            button.add_css_class("suggested-action");
        }
        InstallState::Running { action } => {
            button.set_sensitive(true);
            button.set_label(&format!("Stop {action}"));
            button.remove_css_class("suggested-action");
            button.add_css_class("destructive-action");
        }
        InstallState::Stopping => {
            button.set_sensitive(false);
            button.set_label("Stopping...");
            button.remove_css_class("suggested-action");
            button.add_css_class("destructive-action");
        }
    }
}

fn apply_login_state(button: &gtk::Button, state: LoginState) {
    match state {
        LoginState::Idle => {
            button.set_label("Login");
            button.remove_css_class("destructive-action");
            button.remove_css_class("suggested-action");
        }
        LoginState::Waiting => {
            button.set_label("Cancel Login");
            button.remove_css_class("suggested-action");
            button.add_css_class("destructive-action");
        }
        LoginState::LoggedIn => {
            button.set_label("Logged In");
            button.remove_css_class("destructive-action");
            button.add_css_class("suggested-action");
        }
    }
}

fn apply_launch_state(button: &gtk::Button, state: LaunchState) {
    match state {
        LaunchState::Idle => {
            button.set_sensitive(true);
            button.set_label("Play");
            button.remove_css_class("destructive-action");
            button.add_css_class("suggested-action");
        }
        LaunchState::Running => {
            button.set_sensitive(true);
            button.set_label("Stop");
            button.remove_css_class("suggested-action");
            button.add_css_class("destructive-action");
        }
        LaunchState::Stopping => {
            button.set_sensitive(false);
            button.set_label("Stopping...");
            button.remove_css_class("suggested-action");
            button.add_css_class("destructive-action");
        }
    }
}
