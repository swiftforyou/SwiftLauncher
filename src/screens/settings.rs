use iced::widget::{
    button, checkbox, column, container, row, scrollable, slider, text, text_input, Space,
};
use iced::{Alignment, Element, Length};

use crate::auth::Session;
use crate::messages::Message;
use crate::storage::settings::LauncherSettings;
use crate::theme;

pub fn view<'a>(
    settings: &'a LauncherSettings,
    java_status: &'a str,
    _accounts: &'a [Session],
    _active: Option<&'a Session>,
) -> Element<'a, Message> {
    let header = row![
        column![
            text("Settings")
                .size(24)
                .color(theme::DARK.palette().accent),
            text("Launcher preferences and account controls").size(13),
        ]
        .spacing(4),
        Space::with_width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let body = column![
        section("Java", java(settings, java_status)),
        section("Game", game(settings)),
        section("Integrations", integrations(settings)),
        section("About", about()),
    ]
    .spacing(14);

    let content = column![
        header,
        scrollable(
            container(body)
                .padding(theme::scrollbar_gutter())
                .width(Length::Fill),
        )
        .height(Length::Fill)
        .style(theme::scrollable),
    ]
    .spacing(14);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::app_container)
        .into()
}

fn section<'a>(title: &'static str, body: Element<'a, Message>) -> Element<'a, Message> {
    container(
        column![
            text(title).size(18).color(theme::DARK.palette().accent),
            body
        ]
        .spacing(10),
    )
    .padding(14)
    .style(theme::card)
    .width(Length::Fill)
    .into()
}

fn java<'a>(settings: &'a LauncherSettings, java_status: &'a str) -> Element<'a, Message> {
    column![
        row![
            text_input("Default Java path", &settings.default_java_path)
                .on_input(Message::DefaultJavaChanged)
                .style(theme::input)
                .padding(10),
            button("Choose")
                .on_press(Message::PickDefaultJava)
                .style(theme::secondary_button),
            button("Check")
                .on_press(Message::ValidateDefaultJava)
                .style(theme::secondary_button),
        ]
        .spacing(8),
        row![
            text(format!("Default RAM {} MB", settings.default_ram_mb)).width(Length::Fixed(170.0)),
            slider(
                512..=16384,
                settings.default_ram_mb,
                Message::DefaultRamChanged
            )
            .step(256_u32)
            .style(theme::slider),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        text_input("Global JVM arguments", &settings.global_jvm_args)
            .on_input(Message::GlobalJvmArgsChanged)
            .style(theme::input)
            .padding(10),
        row![
            button("Get Java 8")
                .on_press(Message::DownloadManagedJava(8))
                .style(theme::secondary_button),
            button("Get Java 17")
                .on_press(Message::DownloadManagedJava(17))
                .style(theme::secondary_button),
            button("Get Java 21")
                .on_press(Message::DownloadManagedJava(21))
                .style(theme::secondary_button),
            Space::with_width(Length::Fill),
            button("Open Folder")
                .on_press(Message::OpenManagedJavaDir)
                .style(theme::secondary_button),
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
        row![
            text_input(
                "Default game directory",
                &settings.default_game_dir.display().to_string()
            )
            .on_input(Message::DefaultGameDirChanged)
            .style(theme::input)
            .padding(10),
            button("Choose")
                .on_press(Message::PickDefaultGameDir)
                .style(theme::secondary_button),
        ]
        .spacing(8),
        checkbox("Discord Rich Presence", settings.discord_presence)
            .on_toggle(Message::DiscordPresenceChanged)
            .style(theme::checkbox),
        checkbox("Crash reporter", settings.crash_reporter)
            .on_toggle(Message::CrashReporterChanged)
            .style(theme::checkbox),
    ]
    .spacing(10)
    .into()
}

fn integrations(settings: &LauncherSettings) -> Element<'_, Message> {
    column![
        text("CurseForge API key").size(13),
        text_input(
            "Paste key here for CurseForge modpack imports",
            &settings.curseforge_api_key
        )
        .on_input(Message::CurseForgeApiKeyChanged)
        .secure(true)
        .style(theme::input)
        .padding(10),
        text("Saved locally. Pack players do not need shell exports.").size(12),
    ]
    .spacing(8)
    .into()
}

fn about() -> Element<'static, Message> {
    let commit = option_env!("GIT_HASH").unwrap_or("dev");
    column![
        text(format!("Swift Launcher {}", env!("CARGO_PKG_VERSION"))).size(14),
        text(format!("Commit {commit}")).size(12),
        row![
            button("GitHub")
                .on_press(Message::OpenExternal("https://github.com/".into()))
                .style(theme::secondary_button),
            button("Discord")
                .on_press(Message::OpenExternal("https://discord.com/".into()))
                .style(theme::secondary_button),
            button("Report Bug")
                .on_press(Message::OpenExternal("https://github.com/".into()))
                .style(theme::secondary_button),
        ]
        .spacing(8),
    ]
    .spacing(10)
    .into()
}
