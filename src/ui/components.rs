use super::{app::ProgressState, status::StatusSnapshot};
use hearthstone_linux::config::{AppConfig, Locale, Region};
use relm4::gtk::prelude::*;
use relm4::{gtk, prelude::*, ComponentController, Controller, RelmWidgetExt, Sender};
use relm4_components::simple_combo_box::{SimpleComboBox, SimpleComboBoxMsg};

#[derive(Clone, Debug)]
pub struct StatusPanelState {
    pub snapshot: StatusSnapshot,
    pub progress: ProgressState,
}

pub struct StatusPanel {
    state: StatusPanelState,
}

#[derive(Debug)]
pub enum StatusPanelInput {
    Render(StatusPanelState),
}

#[relm4::component(pub)]
impl SimpleComponent for StatusPanel {
    type Init = StatusPanelState;
    type Input = StatusPanelInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 6,

            gtk::Label {
                set_xalign: 0.0,
                add_css_class: relm4::css::TITLE_3,

                #[watch]
                set_label: &model.state.snapshot.headline,
            },

            gtk::Label {
                set_xalign: 0.0,
                add_css_class: relm4::css::DIM_LABEL,

                #[watch]
                set_label: &model.state.snapshot.details,
            },

            gtk::ProgressBar {
                set_show_text: true,

                #[watch]
                set_visible: model.state.progress.visible,

                #[watch]
                set_fraction: model.state.progress.fraction.unwrap_or_default(),

                #[watch]
                set_text: model.state.progress.text.as_deref(),
            },
        }
    }

    fn init(
        state: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = StatusPanel { state };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        match input {
            StatusPanelInput::Render(state) => self.state = state,
        }
    }
}

pub struct SettingsPanel {
    config: AppConfig,
    region: Controller<SimpleComboBox<Region>>,
    locale: Controller<SimpleComboBox<Locale>>,
}

#[derive(Debug)]
pub enum SettingsPanelInput {
    SetConfig(AppConfig),
    RegionChanged(usize),
    LocaleChanged(usize),
}

#[derive(Clone, Copy, Debug)]
pub enum SettingsOutput {
    RegionChanged(Region),
    LocaleChanged(Locale),
}

#[relm4::component(pub)]
impl SimpleComponent for SettingsPanel {
    type Init = AppConfig;
    type Input = SettingsPanelInput;
    type Output = SettingsOutput;

    view! {
        gtk::Grid {
            set_column_spacing: 12,
            set_row_spacing: 8,

            attach[0, 0, 1, 1] = &gtk::Label {
                set_label: "Region",
                set_xalign: 0.0,
            },

            attach[1, 0, 1, 1] = &gtk::Box {
                #[local_ref]
                region -> gtk::ComboBoxText {},
            },

            attach[0, 1, 1, 1] = &gtk::Label {
                set_label: "Locale",
                set_xalign: 0.0,
            },

            attach[1, 1, 1, 1] = &gtk::Box {
                #[local_ref]
                locale -> gtk::ComboBoxText {},
            },
        }
    }

    fn init(
        config: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let region = SimpleComboBox::builder()
            .launch(SimpleComboBox {
                variants: Region::ALL.to_vec(),
                active_index: index_of(&Region::ALL, config.region),
            })
            .forward(sender.input_sender(), SettingsPanelInput::RegionChanged);
        let locale = SimpleComboBox::builder()
            .launch(SimpleComboBox {
                variants: Locale::ALL.to_vec(),
                active_index: index_of(&Locale::ALL, config.locale),
            })
            .forward(sender.input_sender(), SettingsPanelInput::LocaleChanged);

        let model = SettingsPanel {
            config,
            region,
            locale,
        };

        let region = model.region.widget();
        let locale = model.locale.widget();
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, sender: ComponentSender<Self>) {
        match input {
            SettingsPanelInput::SetConfig(config) => {
                let region_changed = self.config.region != config.region;
                let locale_changed = self.config.locale != config.locale;
                self.config = config;
                if region_changed {
                    self.region
                        .emit(SimpleComboBoxMsg::SetActiveIdx(self.region_index()));
                }
                if locale_changed {
                    self.locale
                        .emit(SimpleComboBoxMsg::SetActiveIdx(self.locale_index()));
                }
            }
            SettingsPanelInput::RegionChanged(idx) => {
                if let Some(region) = Region::ALL.get(idx).copied() {
                    self.config.region = region;
                    sender.output(SettingsOutput::RegionChanged(region)).ok();
                }
            }
            SettingsPanelInput::LocaleChanged(idx) => {
                if let Some(locale) = Locale::ALL.get(idx).copied() {
                    self.config.locale = locale;
                    sender.output(SettingsOutput::LocaleChanged(locale)).ok();
                }
            }
        }
    }
}

impl SettingsPanel {
    fn region_index(&self) -> usize {
        index_of(&Region::ALL, self.config.region).unwrap_or(0)
    }

    fn locale_index(&self) -> usize {
        index_of(&Locale::ALL, self.config.locale).unwrap_or(0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallState {
    Idle,
    Running(String),
    Stopping,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginState {
    Idle,
    Waiting,
    LoggedIn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchState {
    Idle,
    Running,
    Stopping,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionBarState {
    pub install: InstallState,
    pub login: LoginState,
    pub launch: LaunchState,
}

impl Default for ActionBarState {
    fn default() -> Self {
        Self {
            install: InstallState::Idle,
            login: LoginState::Idle,
            launch: LaunchState::Idle,
        }
    }
}

pub struct ActionBar {
    state: ActionBarState,
    account_popover: Option<gtk::Popover>,
}

#[derive(Debug)]
pub enum ActionBarInput {
    Render(ActionBarState),
    ShowAccountMenu,
}

#[derive(Debug)]
pub enum ActionBarOutput {
    Install,
    Login,
    Launch,
    Refresh,
    Logout,
    SwitchAccount,
}

#[relm4::component(pub)]
impl SimpleComponent for ActionBar {
    type Init = ActionBarState;
    type Input = ActionBarInput;
    type Output = ActionBarOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,

            gtk::Button {
                #[watch]
                set_label: &model.install_label(),
                #[watch]
                set_sensitive: model.state.install != InstallState::Stopping,
                #[watch]
                set_class_active: (relm4::css::SUGGESTED_ACTION, model.state.install == InstallState::Idle),
                #[watch]
                set_class_active: (relm4::css::DESTRUCTIVE_ACTION, model.state.install != InstallState::Idle),
                connect_clicked[sender] => move |_| {
                    sender.output(ActionBarOutput::Install).ok();
                },
            },

            #[name(login_button)]
            gtk::Button {
                #[watch]
                set_label: model.login_label(),
                #[watch]
                set_class_active: (relm4::css::SUGGESTED_ACTION, model.state.login == LoginState::LoggedIn),
                #[watch]
                set_class_active: (relm4::css::DESTRUCTIVE_ACTION, model.state.login == LoginState::Waiting),

                connect_clicked[sender] => move |_| {
                    sender.output(ActionBarOutput::Login).ok();
                },
            },

            gtk::Button {
                #[watch]
                set_label: &model.launch_label(),
                #[watch]
                set_sensitive: model.state.launch != LaunchState::Stopping,
                #[watch]
                set_class_active: (relm4::css::SUGGESTED_ACTION, model.state.launch == LaunchState::Idle),
                #[watch]
                set_class_active: (relm4::css::DESTRUCTIVE_ACTION, model.state.launch != LaunchState::Idle),
                connect_clicked[sender] => move |_| {
                    sender.output(ActionBarOutput::Launch).ok();
                },
            },

            gtk::Button {
                set_label: "Refresh",
                connect_clicked[sender] => move |_| {
                    sender.output(ActionBarOutput::Refresh).ok();
                },
            },
        }
    }

    fn init(
        state: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = ActionBar {
            state,
            account_popover: None,
        };
        let widgets = view_output!();
        model.account_popover = Some(account_popover(&widgets.login_button, sender.clone()));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        match input {
            ActionBarInput::Render(state) => self.state = state,
            ActionBarInput::ShowAccountMenu => {
                if let Some(popover) = self.account_popover.as_ref() {
                    popover.popup();
                }
            }
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: Sender<Self::Output>) {
        if let Some(popover) = self.account_popover.take() {
            popover.popdown();
            popover.unparent();
        }
    }
}

impl ActionBar {
    fn install_label(&self) -> String {
        match &self.state.install {
            InstallState::Idle => "Install / Update".into(),
            InstallState::Running(action) => format!("Stop {action}"),
            InstallState::Stopping => "Stopping...".into(),
        }
    }

    fn login_label(&self) -> &'static str {
        match self.state.login {
            LoginState::Idle => "Login",
            LoginState::Waiting => "Cancel Login",
            LoginState::LoggedIn => "Logged In",
        }
    }

    fn launch_label(&self) -> &'static str {
        match self.state.launch {
            LaunchState::Idle => "Play",
            LaunchState::Running => "Stop",
            LaunchState::Stopping => "Stopping...",
        }
    }
}

fn index_of<T: Copy + Eq>(items: &[T], value: T) -> Option<usize> {
    items.iter().position(|item| *item == value)
}

fn account_popover(anchor: &gtk::Button, sender: ComponentSender<ActionBar>) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(anchor);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(10);
    content.set_margin_end(10);

    let switch_account = gtk::Button::with_label("Switch Account");
    {
        let popover = popover.clone();
        let output = sender.output_sender().clone();
        switch_account.connect_clicked(move |_| {
            popover.popdown();
            output.emit(ActionBarOutput::SwitchAccount);
        });
    }

    let logout = gtk::Button::with_label("Log Out");
    logout.add_css_class(relm4::css::DESTRUCTIVE_ACTION);
    {
        let popover = popover.clone();
        let output = sender.output_sender().clone();
        logout.connect_clicked(move |_| {
            popover.popdown();
            output.emit(ActionBarOutput::Logout);
        });
    }

    content.append(&switch_account);
    content.append(&logout);
    popover.set_child(Some(&content));
    popover
}
