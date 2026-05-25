use iced::widget::{button, checkbox, column, container, row, scrollable, slider, text, text_input, Space};
use iced::{Alignment, Element, Length};

use crate::icons::{self, icon_button, svg_icon};
use crate::instances::mods::{InstalledMod, ModrinthProject};
use crate::instances::{Instance, InstanceRunState, InstanceTab};
use crate::messages::Message;
use crate::theme;

pub fn view<'a>(
    instance: &'a Instance,
    tab: InstanceTab,
    mods_search: &'a str,
    mod_import_path: &'a str,
    export_path: &'a str,
    export_busy: bool,
    modrinth_query: &'a str,
    modrinth_results: &'a [ModrinthProject],
    modrinth_busy: bool,
    installed_mods: &'a [InstalledMod],
    mods_loading: bool,
    launch_log: &'a [String],
    launch_status: Option<&'a str>,
) -> Element<'a, Message> {
    let mut header_meta = column![
        text_input("Instance name", &instance.name)
            .on_input(Message::InstanceNameChanged)
            .style(theme::input)
            .padding(10),
        text(format!("Minecraft {} • {}", instance.minecraft_version, instance.loader)).size(13),
    ]
    .spacing(8);
    if let Some(status) = launch_status {
        header_meta = header_meta.push(text(format!("Status: {status}")).size(12));
    }

    let header = row![
        header_meta,
        Space::with_width(Length::Fill),
        icon_button(icons::CLOSE, 18.0, Message::CloseInstanceDetail, theme::secondary_button),
    ]
    .align_y(Alignment::Center);

    let tabs = row![
        tab_button("Overview", InstanceTab::Overview, tab),
        tab_button("Mods", InstanceTab::Mods, tab),
        tab_button("Files", InstanceTab::Files, tab),
        tab_button("Settings", InstanceTab::Settings, tab),
        tab_button("Logs", InstanceTab::Logs, tab),
    ]
    .spacing(8);

    let body = match tab {
        InstanceTab::Overview => overview(instance, launch_log, launch_status),
        InstanceTab::Mods => mods(
            mods_search,
            mod_import_path,
            modrinth_query,
            modrinth_results,
            modrinth_busy,
            installed_mods,
            mods_loading,
        ),
        InstanceTab::Files => files(instance, export_path, export_busy),
        InstanceTab::Settings => settings(instance),
        InstanceTab::Logs => logs(launch_log, launch_status),
    };

    container(column![header, tabs, body].spacing(14))
        .padding(16)
        .style(theme::shell)
        .width(Length::Fill)
        .into()
}

fn tab_button(label: &'static str, target: InstanceTab, selected: InstanceTab) -> iced::widget::Button<'static, Message> {
    let style = if target == selected {
        theme::primary_button
    } else {
        theme::secondary_button
    };
    button(text(label).size(13))
        .on_press(Message::SelectInstanceTab(target))
        .style(style)
        .padding([8, 12])
}

fn overview<'a>(instance: &'a Instance, launch_log: &'a [String], launch_status: Option<&'a str>) -> Element<'a, Message> {
    let last_played = instance
        .last_played_unix
        .map(|v| v.to_string())
        .unwrap_or_else(|| "Never".into());
    let state_label = launch_status
        .map(str::to_string)
        .unwrap_or_else(|| match instance.run_state {
            InstanceRunState::Idle => "Idle".into(),
            InstanceRunState::Preparing => "Launching".into(),
            InstanceRunState::Running => "Running".into(),
        });
    let log_preview = launch_log
        .iter()
        .rev()
        .take(6)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    let log_text = if log_preview.is_empty() {
        "No recent launch output.".into()
    } else {
        log_preview
    };

    column![
        row![
            stat_card("Last played", last_played),
            stat_card("Playtime", format!("{} min", instance.playtime_seconds / 60)),
            stat_card("State", state_label),
        ]
        .spacing(10),
        text(format!("RAM: {} MB", instance.ram_mb)),
        slider(512..=16384, instance.ram_mb, Message::RamChanged).step(256_u32),
        text_input("Java path", &instance.java_path)
            .on_input(Message::JavaPathChanged)
            .style(theme::input)
            .padding(10),
        text_input("JVM args", &instance.jvm_args)
            .on_input(Message::JvmArgsChanged)
            .style(theme::input)
            .padding(10),
        container(scrollable(text(log_text).size(11)).height(Length::Fixed(120.0)))
            .padding(12)
            .style(theme::card),
    ]
    .spacing(12)
    .into()
}

fn stat_card(label: &'static str, value: String) -> Element<'static, Message> {
    container(
        column![text(label).size(11), text(value).size(14)]
            .spacing(4)
            .align_x(Alignment::Start),
    )
    .padding(10)
    .width(Length::FillPortion(1))
    .style(theme::card)
    .into()
}

fn mods<'a>(
    mods_search: &'a str,
    mod_import_path: &'a str,
    modrinth_query: &'a str,
    modrinth_results: &'a [ModrinthProject],
    modrinth_busy: bool,
    installed_mods: &'a [InstalledMod],
    loading: bool,
) -> Element<'a, Message> {
    let search = mods_search.to_lowercase();
    let filtered = installed_mods
        .iter()
        .filter(|item| item.name.to_lowercase().contains(&search))
        .collect::<Vec<_>>();

    let mut list = column![].spacing(8);
    if loading {
        list = list.push(loading_row("Reading mods..."));
    } else if filtered.is_empty() {
        list = list.push(text("No mods installed").size(13));
    } else {
        for item in filtered {
            list = list.push(mod_row(item));
        }
    }

    column![
        row![
            text_input("Search Modrinth", modrinth_query)
                .on_input(Message::ModrinthSearchChanged)
                .style(theme::input)
                .padding(10),
            button(if modrinth_busy { "Searching..." } else { "Search" })
                .on_press(if modrinth_busy { Message::Noop } else { Message::SearchModrinth })
                .style(theme::primary_button),
        ]
        .spacing(8),
        modrinth_results_view(modrinth_results, modrinth_busy),
        row![
            text_input("Search mods", mods_search)
                .on_input(Message::ModsSearchChanged)
                .style(theme::input)
                .padding(10),
            icon_button(icons::ADD, 18.0, Message::AddMod, theme::primary_button),
        ]
        .spacing(8),
        row![
            text_input("/path/to/mod.jar", mod_import_path)
                .on_input(Message::ModImportPathChanged)
                .style(theme::input)
                .padding(10),
            icon_button(icons::FOLDER, 18.0, Message::PickModJar, theme::secondary_button),
            button("Import").on_press(Message::ImportModSubmit).style(theme::secondary_button),
        ]
        .spacing(8),
        scrollable(list).height(Length::Fixed(260.0)),
    ]
    .spacing(12)
    .into()
}

fn loading_row(message: &str) -> Element<'_, Message> {
    container(
        row![
            svg_icon(icons::ALERT, 16.0),
            text(message).size(13),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(8)
    .style(theme::badge)
    .into()
}

fn modrinth_results_view(results: &[ModrinthProject], busy: bool) -> Element<'_, Message> {
    let mut list = column![].spacing(8);
    if busy {
        list = list.push(loading_row("Searching Modrinth..."));
    } else if results.is_empty() {
        list = list.push(text("No results").size(13));
    } else {
        for item in results.iter().take(5) {
            list = list.push(
                container(
                    row![
                        column![
                            text(&item.title).size(14),
                            text(format!("{} downloads • {}", item.downloads, item.description)).size(11),
                        ]
                        .spacing(2),
                        Space::with_width(Length::Fill),
                        button("Install")
                            .on_press(Message::InstallModrinthProject(item.project_id.clone()))
                            .style(theme::secondary_button),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding(10)
                .style(theme::card),
            );
        }
    }
    scrollable(list).height(Length::Fixed(150.0)).into()
}

fn mod_row(item: &InstalledMod) -> Element<'_, Message> {
    let id = item.id.clone();
    container(
        row![
            checkbox("", item.enabled).on_toggle(move |enabled| Message::ToggleMod { mod_id: id.clone(), enabled }),
            column![
                text(&item.name).size(14),
                text(format!(
                    "{} • {}",
                    item.version,
                    if item.enabled { "enabled" } else { "disabled" }
                ))
                .size(11),
            ]
            .spacing(2),
            Space::with_width(Length::Fill),
            icon_button(icons::DELETE, 16.0, Message::DeleteMod(item.id.clone()), theme::danger_button),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding(10)
    .style(theme::card)
    .into()
}

fn files<'a>(instance: &'a Instance, export_path: &'a str, export_busy: bool) -> Element<'a, Message> {
    column![
        text(instance.path.display().to_string()).size(12),
        row![
            icon_button(icons::FOLDER, 18.0, Message::OpenInstanceFiles(instance.id.clone()), theme::secondary_button),
            button("Logs").on_press(Message::OpenInstanceLogs(instance.id.clone())).style(theme::secondary_button),
            button("Crash Reports")
                .on_press(Message::OpenInstanceCrashReports(instance.id.clone()))
                .style(theme::secondary_button),
            button("Screenshots")
                .on_press(Message::OpenInstanceScreenshots(instance.id.clone()))
                .style(theme::secondary_button),
            button("Resource Packs")
                .on_press(Message::OpenInstanceResourcePacks(instance.id.clone()))
                .style(theme::secondary_button),
        ]
        .spacing(8),
        row![
            text_input("/path/to/export.zip", export_path)
                .on_input(Message::ExportPathChanged)
                .style(theme::input)
                .padding(10),
            icon_button(icons::FOLDER, 18.0, Message::PickExportZip(instance.id.clone()), theme::secondary_button),
            button(if export_busy { "Exporting..." } else { "Export Zip" })
                .on_press(if export_busy {
                    Message::Noop
                } else {
                    Message::ExportInstance(instance.id.clone())
                })
                .style(theme::secondary_button),
        ]
        .spacing(8),
    ]
    .spacing(12)
    .into()
}

fn logs<'a>(launch_log: &'a [String], launch_status: Option<&str>) -> Element<'a, Message> {
    let status_line = launch_status
        .map(|status| format!("Current status: {status}"))
        .unwrap_or_else(|| "Current status: idle".into());

    let body: Element<'a, Message> = if launch_log.is_empty() {
        container(
            column![
                svg_icon(icons::FOLDER, 40.0),
                text("No logs yet").size(16),
                text("Launch the instance to capture stdout and stderr here.").size(12),
            ]
            .spacing(10)
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    } else {
        let mut col = column![text(status_line).size(12)].spacing(8);
        for line in launch_log.iter() {
            let style = if line.contains("stderr:") || line.to_ascii_lowercase().contains("error") {
                theme::DARK.palette().danger
            } else if line.to_ascii_lowercase().contains("warn") {
                theme::DARK.palette().warning
            } else {
                theme::DARK.palette().text
            };
            col = col.push(
                container(text(line).size(11).color(style))
                    .padding([2, 0])
                    .width(Length::Fill),
            );
        }
        scrollable(col).height(Length::Fill).into()
    };

    column![
        row![
            text("Launch log").size(16),
            Space::with_width(Length::Fill),
            button("Copy log").on_press(Message::CopyLogs).style(theme::secondary_button),
        ]
        .align_y(Alignment::Center),
        container(body)
            .padding(12)
            .style(theme::card)
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .spacing(10)
    .height(Length::Fill)
    .into()
}

fn settings(instance: &Instance) -> Element<'_, Message> {
    column![
        row![
            text_input("Width", &instance.resolution_width.to_string())
                .on_input(Message::ResolutionWidthChanged)
                .style(theme::input)
                .padding(10),
            text_input("Height", &instance.resolution_height.to_string())
                .on_input(Message::ResolutionHeightChanged)
                .style(theme::input)
                .padding(10),
        ]
        .spacing(8),
        checkbox("Fullscreen", instance.fullscreen).on_toggle(Message::FullscreenChanged),
        text_input("Game directory override", &instance.game_dir_override)
            .on_input(Message::GameDirOverrideChanged)
            .style(theme::input)
            .padding(10),
        text_input("Server host:port", &instance.server)
            .on_input(Message::ServerChanged)
            .style(theme::input)
            .padding(10),
    ]
    .spacing(12)
    .into()
}
