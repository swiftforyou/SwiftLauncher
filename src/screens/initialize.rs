use iced::widget::{
    button, checkbox, column, container, row, slider, svg, text, text_input, Space,
};
use iced::{Alignment, Element, Length};

use crate::icons;
use crate::messages::Message;
use crate::storage::settings::LauncherSettings;
use crate::theme;

const LAST_STEP: u8 = 4;

pub fn view<'a>(
    settings: &'a LauncherSettings,
    desktop_integration: bool,
    busy: bool,
    banner: Option<&'a str>,
    window_width: f32,
    step: u8,
) -> Element<'a, Message> {
    let step = step.min(LAST_STEP);
    let compact = window_width < 720.0;
    let panel_width = if compact { 520 } else { 680 };

    let mut content = column![wizard_panel(
        settings,
        desktop_integration,
        busy,
        compact,
        step
    )]
    .spacing(14)
    .align_x(Alignment::Center);

    if let Some(message) = banner {
        content = column![
            container(text(message).size(13))
                .padding(12)
                .width(Length::Fill)
                .style(theme::banner),
            content,
        ]
        .spacing(14)
        .align_x(Alignment::Center);
    }

    container(
        container(content)
            .width(Length::Fill)
            .max_width(panel_width)
            .center_x(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(if compact { [18, 14] } else { [28, 24] })
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(theme::app_container)
    .into()
}

fn wizard_panel<'a>(
    settings: &'a LauncherSettings,
    desktop_integration: bool,
    busy: bool,
    compact: bool,
    step: u8,
) -> Element<'a, Message> {
    container(
        column![
            wizard_header(step, compact),
            step_body(settings, desktop_integration, compact, step),
            footer_nav(step, busy),
        ]
        .spacing(if compact { 14 } else { 18 }),
    )
    .padding(if compact { 16 } else { 22 })
    .width(Length::Fill)
    .style(theme::shell)
    .into()
}

fn wizard_header(step: u8, compact: bool) -> Element<'static, Message> {
    let title = match step {
        0 => "Welcome",
        1 => "Library",
        2 => "Integration",
        3 => "Defaults",
        _ => "Ready",
    };
    let subtitle = match step {
        0 => "Swift Launcher setup",
        1 => "Choose where Minecraft files live",
        2 => "Register with your desktop",
        3 => "Tune launch behavior",
        _ => "Review and continue",
    };
    let logo_size = if compact { 36.0 } else { 44.0 };

    column![
        row![
            svg(svg::Handle::from_memory(icons::LOGO))
                .width(Length::Fixed(logo_size))
                .height(Length::Fixed(logo_size)),
            column![
                text(title).size(if compact { 24 } else { 28 }),
                text(subtitle).size(13).color(theme::DARK.palette().muted),
            ]
            .spacing(3),
            Space::with_width(Length::Fill),
            status_badge(platform_name()),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        progress_steps(step),
    ]
    .spacing(14)
    .into()
}

fn progress_steps(step: u8) -> Element<'static, Message> {
    let labels = ["Start", "Folder", "Desktop", "Defaults", "Finish"];
    let mut row = row![].spacing(8).align_y(Alignment::Center);
    for (index, label) in labels.into_iter().enumerate() {
        let index = index as u8;
        let style = if index <= step {
            theme::active_badge
        } else {
            theme::badge
        };
        row = row.push(container(text(label).size(11)).padding([5, 9]).style(style));
    }
    row.into()
}

fn step_body<'a>(
    settings: &'a LauncherSettings,
    desktop_integration: bool,
    compact: bool,
    step: u8,
) -> Element<'a, Message> {
    match step {
        0 => welcome_step(compact),
        1 => folder_step(settings, compact),
        2 => integration_step(desktop_integration),
        3 => defaults_step(settings, compact),
        _ => finish_step(settings),
    }
}

fn welcome_step(compact: bool) -> Element<'static, Message> {
    let logo_size = if compact { 86.0 } else { 116.0 };
    container(
        column![
            svg(svg::Handle::from_memory(icons::LOGO))
                .width(Length::Fixed(logo_size))
                .height(Length::Fixed(logo_size)),
            text("Welcome to Swift Launcher").size(if compact { 28 } else { 34 }),
            text("A native Minecraft launcher for instances, mods, worlds, servers, and fast launches.")
                .size(14)
                .width(Length::Fill),
            column![
                feature_line("No browser shell, no webview"),
                feature_line("Modrinth, CurseForge, Microsoft, Ely.by, LittleSkin"),
                feature_line(platform_note()),
            ]
            .spacing(9),
        ]
        .spacing(14)
        .align_x(Alignment::Center),
    )
    .padding(if compact { 12 } else { 18 })
    .style(theme::card)
    .width(Length::Fill)
    .into()
}

fn folder_step<'a>(settings: &'a LauncherSettings, compact: bool) -> Element<'a, Message> {
    let data_root = crate::storage::data_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|error| error.to_string());
    let input = text_input(
        "Choose Minecraft library folder",
        &settings.default_game_dir.display().to_string(),
    )
    .on_input(Message::DefaultGameDirChanged)
    .style(theme::input)
    .padding(10);
    let choose = button("Choose")
        .on_press(Message::PickDefaultGameDir)
        .style(theme::secondary_button)
        .padding([10, 12]);
    let picker: Element<'a, Message> = if compact {
        column![input, choose.width(Length::Fill)].spacing(8).into()
    } else {
        row![input, choose]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
    };

    card_body(
        "Game files, instances, mods, resource packs, shaders, saves, and logs are stored in this library folder.",
        column![picker, info_line_owned("Launcher data", data_root)]
            .spacing(10)
            .into(),
    )
}

fn integration_step<'a>(_desktop_integration: bool) -> Element<'a, Message> {
    let mut body = column![text(platform_integration_note()).size(13)].spacing(12);
    #[cfg(target_os = "linux")]
    {
        body = body.push(
            checkbox(
                "Add Swift Launcher to application menu",
                _desktop_integration,
            )
            .on_toggle(Message::SetupDesktopIntegrationChanged)
            .style(theme::checkbox),
        );
    }
    #[cfg(target_os = "windows")]
    {
        body = body.push(
            checkbox("Create desktop shortcut", _desktop_integration)
                .on_toggle(Message::SetupDesktopIntegrationChanged)
                .style(theme::checkbox),
        );
    }

    card_body(
        "This step is OS-aware. Linux can create an app menu entry. Windows can create a desktop shortcut.",
        body.into(),
    )
}

fn defaults_step<'a>(settings: &'a LauncherSettings, compact: bool) -> Element<'a, Message> {
    let checks: Element<'a, Message> = if compact {
        column![
            checkbox("Crash reports", settings.crash_reporter)
                .on_toggle(Message::CrashReporterChanged)
                .style(theme::checkbox),
            checkbox("Discord Rich Presence", settings.discord_presence)
                .on_toggle(Message::DiscordPresenceChanged)
                .style(theme::checkbox),
        ]
        .spacing(8)
        .into()
    } else {
        row![
            checkbox("Crash reports", settings.crash_reporter)
                .on_toggle(Message::CrashReporterChanged)
                .style(theme::checkbox),
            checkbox("Discord Rich Presence", settings.discord_presence)
                .on_toggle(Message::DiscordPresenceChanged)
                .style(theme::checkbox),
        ]
        .spacing(18)
        .align_y(Alignment::Center)
        .into()
    };

    card_body(
        "These defaults apply to new instances. Existing instances can still override them later.",
        column![
            row![
                text("Default RAM").size(13).width(Length::Fixed(112.0)),
                slider(
                    512..=16384,
                    settings.default_ram_mb,
                    Message::DefaultRamChanged,
                )
                .step(256_u32)
                .style(theme::slider),
                text(format!("{} MB", settings.default_ram_mb))
                    .size(12)
                    .width(Length::Fixed(74.0)),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text_input("Java executable", &settings.default_java_path)
                .on_input(Message::DefaultJavaChanged)
                .style(theme::input)
                .padding(10),
            checks,
        ]
        .spacing(12)
        .into(),
    )
}

fn finish_step<'a>(settings: &'a LauncherSettings) -> Element<'a, Message> {
    let library = settings.default_game_dir.display().to_string();
    card_body(
        "Setup is ready. Next screen asks you to sign in, unless you already have an active account.",
        column![
            info_line_owned("Library folder", library),
            info_line("Java", &settings.default_java_path),
            info_line_owned("Default RAM", format!("{} MB", settings.default_ram_mb)),
        ]
        .spacing(10)
        .into(),
    )
}

fn footer_nav(step: u8, busy: bool) -> Element<'static, Message> {
    let back = button("Back")
        .style(theme::secondary_button)
        .padding([10, 14]);
    let back = if step == 0 || busy {
        back
    } else {
        back.on_press(Message::SetupBack)
    };

    let next_label = if step >= LAST_STEP {
        if busy {
            "Saving..."
        } else {
            "Finish Setup"
        }
    } else {
        "Next"
    };
    let next = button(text(next_label).size(14))
        .style(theme::primary_button)
        .padding([10, 16]);
    let next = if busy {
        next
    } else if step >= LAST_STEP {
        next.on_press(Message::FinishSetup)
    } else {
        next.on_press(Message::SetupNext)
    };

    row![back, Space::with_width(Length::Fill), next]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}

fn card_body<'a>(copy: &'static str, body: Element<'a, Message>) -> Element<'a, Message> {
    container(column![text(copy).size(13), body].spacing(14))
        .padding(14)
        .style(theme::card)
        .width(Length::Fill)
        .into()
}

fn feature_line(label: &'static str) -> Element<'static, Message> {
    row![
        container(text("").size(1))
            .width(Length::Fixed(8.0))
            .height(Length::Fixed(8.0))
            .style(theme::active_badge),
        text(label).size(13),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn info_line<'a>(label: &'static str, value: &'a str) -> Element<'a, Message> {
    container(
        column![
            text(label).size(11).color(theme::DARK.palette().muted),
            text(value).size(12),
        ]
        .spacing(3),
    )
    .padding(10)
    .style(theme::toolbar)
    .width(Length::Fill)
    .into()
}

fn info_line_owned<'a>(label: &'static str, value: String) -> Element<'a, Message> {
    container(
        column![
            text(label).size(11).color(theme::DARK.palette().muted),
            text(value).size(12),
        ]
        .spacing(3),
    )
    .padding(10)
    .style(theme::toolbar)
    .width(Length::Fill)
    .into()
}

fn status_badge(label: &'static str) -> Element<'static, Message> {
    container(text(label).size(11))
        .padding([5, 9])
        .style(theme::badge)
        .into()
}

fn platform_name() -> &'static str {
    if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Unsupported OS"
    }
}

fn platform_note() -> &'static str {
    if cfg!(target_os = "linux") {
        "Linux app menu entry available during setup"
    } else if cfg!(target_os = "windows") {
        "Windows desktop shortcut available during setup"
    } else {
        "Only Linux and Windows builds are supported"
    }
}

fn platform_integration_note() -> &'static str {
    if cfg!(target_os = "linux") {
        "Swift can register itself in your desktop application list using the current executable and bundled logo."
    } else if cfg!(target_os = "windows") {
        "Swift can create a desktop shortcut pointing to this executable."
    } else {
        "This build target is not officially supported."
    }
}
