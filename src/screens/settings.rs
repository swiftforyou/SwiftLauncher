use iced::widget::{button, checkbox, column, container, pick_list, row, scrollable, slider, text, text_input, Space};
use iced::{Alignment, Element, Length};

use crate::auth::Session;
use crate::messages::Message;
use crate::storage::settings::LauncherSettings;
use crate::theme::{self, Accent, ThemeMode};

fn styled_pick_list<'a, T, L, V>(
    options: L,
    selected: Option<V>,
    on_selected: impl Fn(T) -> Message + 'a,
) -> iced::widget::PickList<'a, T, L, V, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    L: std::borrow::Borrow<[T]> + 'a,
    V: std::borrow::Borrow<T> + 'a,
{
    pick_list(options, selected, on_selected)
        .style(theme::pick_list)
        .menu_style(theme::pick_list_menu)
}

pub fn view<'a>(
    settings: &'a LauncherSettings,
    java_status: &'a str,
    accounts: &'a [Session],
    active: Option<&'a Session>,
) -> Element<'a, Message> {
    let header = row![
        button("Close").on_press(Message::SettingsClosed).style(theme::secondary_button),
        column![
            text("Settings").size(24),
            text("Launcher preferences and account controls").size(13),
        ]
        .spacing(4),
        Space::with_width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let content = column![
        header,
        section("Appearance", appearance(settings)),
        section("Accounts", accounts_section(accounts, active)),
        section("Java", java(settings, java_status)),
        section("Game", game(settings)),
        section("About", about()),
    ]
    .spacing(14)
    .padding(18);

    container(scrollable(content))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::scrim)
        .into()
}

fn section<'a>(title: &'static str, body: Element<'a, Message>) -> Element<'a, Message> {
    container(column![text(title).size(18), body].spacing(10))
        .padding(14)
        .style(theme::card)
        .width(Length::Fill)
        .into()
}

fn appearance(settings: &LauncherSettings) -> Element<'_, Message> {
    column![
        row![
            text("Theme").width(Length::Fixed(140.0)),
            styled_pick_list([ThemeMode::Dark, ThemeMode::Light, ThemeMode::System], Some(settings.theme_mode), Message::ThemeModeChanged),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text("Accent").width(Length::Fixed(140.0)),
            styled_pick_list(
                [Accent::Indigo, Accent::Green, Accent::Orange, Accent::Pink, Accent::Cyan, Accent::Red],
                Some(settings.accent),
                Message::AccentChanged,
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        row![
            text(format!("UI scale {}%", settings.ui_scale)).width(Length::Fixed(140.0)),
            slider(75..=150, settings.ui_scale, Message::UiScaleChanged).step(25_u16),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(10)
    .into()
}

fn accounts_section<'a>(accounts: &'a [Session], active: Option<&'a Session>) -> Element<'a, Message> {
    let mut list = column![].spacing(8);
    if accounts.is_empty() {
        list = list.push(text("No saved accounts yet").size(13));
    }
    for account in accounts {
        let is_active = active.is_some_and(|session| session.uuid == account.uuid);
        list = list.push(
            row![
                column![
                    text(format!("{}{}", account.username, if is_active { " (active)" } else { "" })).size(14),
                    text(format!("{} • {}", account.provider, account.uuid)).size(11),
                ]
                .spacing(2),
                Space::with_width(Length::Fill),
                button("Use").on_press(Message::AccountSelected(account.uuid.clone())).style(theme::secondary_button),
                button("Remove").on_press(Message::SignOut(account.uuid.clone())).style(theme::danger_button),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }

    list.push(button("Add Account").on_press(Message::AddAccount).style(theme::primary_button))
        .into()
}

fn java<'a>(settings: &'a LauncherSettings, java_status: &'a str) -> Element<'a, Message> {
    column![
        row![
            text_input("Default Java path", &settings.default_java_path)
                .on_input(Message::DefaultJavaChanged)
                .style(theme::input)
                .padding(10),
            button("Choose").on_press(Message::PickDefaultJava).style(theme::secondary_button),
            button("Check").on_press(Message::ValidateDefaultJava).style(theme::secondary_button),
        ]
        .spacing(8),
        row![
            text(format!("Default RAM {} MB", settings.default_ram_mb)).width(Length::Fixed(170.0)),
            slider(512..=16384, settings.default_ram_mb, Message::DefaultRamChanged).step(256_u32),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        text_input("Global JVM arguments", &settings.global_jvm_args)
            .on_input(Message::GlobalJvmArgsChanged)
            .style(theme::input)
            .padding(10),
        row![
            button("Get Java 8").on_press(Message::DownloadManagedJava(8)).style(theme::secondary_button),
            button("Get Java 17").on_press(Message::DownloadManagedJava(17)).style(theme::secondary_button),
            button("Get Java 21").on_press(Message::DownloadManagedJava(21)).style(theme::secondary_button),
            Space::with_width(Length::Fill),
            button("Open Folder").on_press(Message::OpenManagedJavaDir).style(theme::secondary_button),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        text(java_status).size(12),
    ]
    .spacing(10)
    .into()
}

fn game(settings: &LauncherSettings) -> Element<'_, Message> {
    column![
        text_input("Default game directory", &settings.default_game_dir.display().to_string())
            .on_input(Message::DefaultGameDirChanged)
            .style(theme::input)
            .padding(10),
        checkbox("Discord Rich Presence", settings.discord_presence).on_toggle(Message::DiscordPresenceChanged),
        checkbox("Crash reporter", settings.crash_reporter).on_toggle(Message::CrashReporterChanged),
    ]
    .spacing(10)
    .into()
}

fn about() -> Element<'static, Message> {
    let commit = option_env!("GIT_HASH").unwrap_or("dev");
    column![
        text(format!("Swift Launcher {}", env!("CARGO_PKG_VERSION"))).size(14),
        text(format!("Commit {commit}")).size(12),
        row![
            button("GitHub").on_press(Message::OpenExternal("https://github.com/".into())).style(theme::secondary_button),
            button("Discord").on_press(Message::OpenExternal("https://discord.com/".into())).style(theme::secondary_button),
            button("Report Bug").on_press(Message::OpenExternal("https://github.com/".into())).style(theme::secondary_button),
        ]
        .spacing(8),
    ]
    .spacing(10)
    .into()
}
