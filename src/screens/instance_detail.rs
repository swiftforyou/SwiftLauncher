use iced::widget::{
    button, checkbox, column, container, image, markdown, pick_list, progress_bar, row, scrollable,
    slider, text, text_input, Space,
};
use iced::{Alignment, Element, Font, Length, Theme as IcedTheme};

use crate::icons::{self, icon_button, icon_label_button, svg_icon};
use crate::instances::mods::{
    InstalledMod, ModrinthKind, ModrinthProject, ModrinthProjectDetail, ResourceProvider,
};
use crate::instances::{Instance, InstanceRunState, InstanceTab};
use crate::messages::Message;
use crate::theme;

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    instance: &'a Instance,
    tab: InstanceTab,
    mods_search: &'a str,
    mod_import_path: &'a str,
    export_path: &'a str,
    export_busy: bool,
    modrinth_query: &'a str,
    resource_provider: ResourceProvider,
    modrinth_kind: ModrinthKind,
    modrinth_results: &'a [ModrinthProject],
    modrinth_detail: Option<&'a ModrinthProjectDetail>,
    modrinth_markdown: &'a [markdown::Item],
    modrinth_detail_busy: bool,
    modrinth_busy: bool,
    installed_mods: &'a [InstalledMod],
    mod_categories: &'a [String],
    new_mod_category: &'a str,
    mods_loading: bool,
    modrinth_install_status: &'a str,
    modrinth_install_progress: f32,
    launch_log: &'a [String],
    launch_status: Option<&'a str>,
    launch_progress: Option<f32>,
) -> Element<'a, Message> {
    let mut header_meta = column![
        text_input("Instance name", &instance.name)
            .on_input(Message::InstanceNameChanged)
            .style(theme::input)
            .padding(10),
        text(format!(
            "Minecraft {} • {}",
            instance.minecraft_version, instance.loader
        ))
        .size(13),
    ]
    .spacing(8);
    if let Some(status) = launch_status {
        header_meta = header_meta.push(text(format!("Status: {status}")).size(12));
    }
    if let Some(progress) = launch_progress {
        header_meta = header_meta.push(
            progress_bar(0.0..=1.0, progress)
                .height(Length::Fixed(5.0))
                .style(theme::progress),
        );
    }

    let header = row![
        header_meta,
        Space::with_width(Length::Fill),
        icon_button(
            icons::CLOSE,
            18.0,
            Message::CloseInstanceDetail,
            theme::secondary_button
        ),
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
        InstanceTab::Overview => overview(instance, launch_log, launch_status, launch_progress),
        InstanceTab::Mods => mods(
            mods_search,
            mod_import_path,
            modrinth_query,
            resource_provider,
            modrinth_kind,
            modrinth_results,
            modrinth_detail,
            modrinth_markdown,
            modrinth_detail_busy,
            modrinth_busy,
            installed_mods,
            mod_categories,
            new_mod_category,
            mods_loading,
            modrinth_install_status,
            modrinth_install_progress,
        ),
        InstanceTab::Files => files(instance, export_path, export_busy),
        InstanceTab::Settings => settings(instance),
        InstanceTab::Logs => logs(launch_log, launch_status, launch_progress),
    };

    let body = container(body)
        .height(Length::Fill)
        .width(Length::Fill)
        .clip(true);

    container(column![header, tabs, body].spacing(14).height(Length::Fill))
        .padding(16)
        .style(theme::shell)
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .into()
}

fn tab_button(
    label: &'static str,
    target: InstanceTab,
    selected: InstanceTab,
) -> iced::widget::Button<'static, Message> {
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

fn overview<'a>(
    instance: &'a Instance,
    launch_log: &'a [String],
    launch_status: Option<&'a str>,
    launch_progress: Option<f32>,
) -> Element<'a, Message> {
    let last_played = instance
        .last_played_unix
        .map(|v| v.to_string())
        .unwrap_or_else(|| "Never".into());
    let state_label =
        launch_status
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

    let mut content = column![row![
        stat_card("Last played", last_played),
        stat_card(
            "Playtime",
            format!("{} min", instance.playtime_seconds / 60)
        ),
        stat_card("State", state_label),
    ]
    .spacing(10),]
    .spacing(12);
    if let Some(progress) = launch_progress {
        content = content.push(
            container(
                column![
                    text("Launch progress").size(12),
                    progress_bar(0.0..=1.0, progress)
                        .height(Length::Fixed(6.0))
                        .style(theme::progress),
                ]
                .spacing(8),
            )
            .padding(12)
            .style(theme::card),
        );
    }
    content = content.push(
        column![
            text(format!("RAM: {} MB", instance.ram_mb)),
            slider(512..=16384, instance.ram_mb, Message::RamChanged)
                .step(256_u32)
                .style(theme::slider),
            text_input("Java path", &instance.java_path)
                .on_input(Message::JavaPathChanged)
                .style(theme::input)
                .padding(10),
            text_input("JVM args", &instance.jvm_args)
                .on_input(Message::JvmArgsChanged)
                .style(theme::input)
                .padding(10),
            container(
                scrollable(
                    container(text(log_text).size(11).font(Font::MONOSPACE))
                        .padding(theme::scrollbar_gutter())
                        .width(Length::Fill),
                )
                .style(theme::scrollable)
                .height(Length::Fixed(120.0)),
            )
            .padding(12)
            .style(theme::card),
        ]
        .spacing(12),
    );
    content.into()
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

#[allow(clippy::too_many_arguments)]
fn mods<'a>(
    mods_search: &'a str,
    mod_import_path: &'a str,
    _modrinth_query: &'a str,
    _resource_provider: ResourceProvider,
    _modrinth_kind: ModrinthKind,
    _modrinth_results: &'a [ModrinthProject],
    _modrinth_detail: Option<&'a ModrinthProjectDetail>,
    _modrinth_markdown: &'a [markdown::Item],
    _modrinth_detail_busy: bool,
    _modrinth_busy: bool,
    installed_mods: &'a [InstalledMod],
    mod_categories: &'a [String],
    new_mod_category: &'a str,
    loading: bool,
    modrinth_install_status: &'a str,
    modrinth_install_progress: f32,
) -> Element<'a, Message> {
    let search = mods_search.to_lowercase();
    let filtered = installed_mods
        .iter()
        .filter(|item| item.name.to_lowercase().contains(&search))
        .collect::<Vec<_>>();

    let mut list = column![].spacing(8);
    if loading {
        let status = if modrinth_install_status.trim().is_empty() {
            "Reading mods..."
        } else {
            modrinth_install_status
        };
        list = list.push(
            container(
                column![
                    loading_row(status),
                    progress_bar(0.0..=1.0, modrinth_install_progress)
                        .height(Length::Fixed(6.0))
                        .style(theme::progress),
                ]
                .spacing(8),
            )
            .padding(10)
            .style(theme::card),
        );
    } else if filtered.is_empty() {
        list = list.push(text("No mods installed").size(13));
    } else {
        let mut current_category = String::new();
        for item in filtered {
            if item.category != current_category {
                current_category = item.category.clone();
                list = list.push(text(current_category.clone()).size(15));
            }
            list = list.push(mod_row(item, mod_categories));
        }
    }

    let content = column![
        row![
            column![
                text("Local Mods").size(22),
                text(
                    "Manage installed files only. Search and download new resources from Discover."
                )
                .size(12),
            ]
            .spacing(4),
            Space::with_width(Length::Fill),
        ]
        .align_y(Alignment::Center),
        row![
            text_input("Search mods", mods_search)
                .on_input(Message::ModsSearchChanged)
                .style(theme::input)
                .padding(10),
            icon_button(icons::ADD, 18.0, Message::AddMod, theme::primary_button),
        ]
        .spacing(8),
        row![
            text_input("New category", new_mod_category)
                .on_input(Message::ModCategoryNameChanged)
                .style(theme::input)
                .padding(10),
            button("Add Category")
                .on_press(Message::AddModCategory)
                .style(theme::secondary_button),
        ]
        .spacing(8),
        row![
            text_input("/path/to/mod.jar", mod_import_path)
                .on_input(Message::ModImportPathChanged)
                .style(theme::input)
                .padding(10),
            icon_button(
                icons::FOLDER,
                18.0,
                Message::PickModJar,
                theme::secondary_button
            ),
            button("Import")
                .on_press(Message::ImportModSubmit)
                .style(theme::secondary_button),
        ]
        .spacing(8),
        list,
    ]
    .spacing(12);

    scrollable(container(content).padding([0, 18]).width(Length::Fill))
        .height(Length::Fill)
        .width(Length::Fill)
        .style(theme::scrollable)
        .into()
}

fn modrinth_kind_selector(selected: ModrinthKind) -> Element<'static, Message> {
    let mut line = row![].spacing(8);
    for kind in ModrinthKind::ALL {
        let style = if kind == selected {
            theme::primary_button
        } else {
            theme::secondary_button
        };
        line = line.push(
            button(text(kind.to_string()).size(12))
                .on_press(Message::ModrinthKindSelected(kind))
                .style(style)
                .padding([8, 10]),
        );
    }
    line.into()
}

fn loading_row(message: &str) -> Element<'_, Message> {
    container(
        row![svg_icon(icons::ALERT, 16.0), text(message).size(13),]
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
        list = list.push(loading_row("Searching..."));
    } else if results.is_empty() {
        list = list.push(text("No results").size(13));
    } else {
        for item in results.iter().take(8) {
            list = list.push(
                container(
                    row![
                        project_icon(item.icon.as_ref(), 42.0),
                        column![
                            text(&item.title).size(14),
                            row![
                                badge_text(format_downloads(item.downloads)),
                                text(&item.description).size(11),
                            ]
                            .spacing(6),
                        ]
                        .spacing(2),
                        Space::with_width(Length::Fill),
                        button("Open")
                            .on_press(Message::OpenModrinthProject(item.project_id.clone()))
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
    list.into()
}

fn modrinth_detail_view<'a>(
    detail: &'a ModrinthProjectDetail,
    parsed_markdown: &'a [markdown::Item],
    installing: bool,
    install_status: &'a str,
    install_progress: f32,
) -> Element<'a, Message> {
    let mut body = column![row![
        project_icon(detail.icon.as_ref(), 58.0),
        column![
            text(&detail.title).size(20),
            row![
                badge_text(detail.kind.to_string()),
                badge_text(format_downloads(detail.downloads))
            ]
            .spacing(6),
            text(&detail.description).size(12),
        ]
        .spacing(5),
        Space::with_width(Length::Fill),
        icon_label_button(
            icons::BACK,
            16.0,
            "Back",
            Message::CloseModrinthProject,
            theme::secondary_button
        ),
        install_resource_button(detail.project_id.clone(), installing),
    ]
    .spacing(12)
    .align_y(Alignment::Center),]
    .spacing(12);

    if installing {
        body = body.push(
            container(
                column![
                    loading_row(if install_status.trim().is_empty() {
                        "Installing..."
                    } else {
                        install_status
                    }),
                    progress_bar(0.0..=1.0, install_progress)
                        .height(Length::Fixed(6.0))
                        .style(theme::progress),
                ]
                .spacing(8),
            )
            .padding(10)
            .style(theme::card),
        );
    }

    if !detail.gallery.is_empty() {
        let mut gallery = row![].spacing(8);
        for bytes in &detail.gallery {
            gallery = gallery.push(
                image(image::Handle::from_bytes(bytes.clone()))
                    .width(Length::Fixed(150.0))
                    .height(Length::Fixed(90.0))
                    .content_fit(iced::ContentFit::Cover),
            );
        }
        body = body.push(gallery);
    }

    let markdown_body = markdown::view(
        parsed_markdown,
        markdown::Settings::with_text_size(13),
        markdown::Style::from_palette(IcedTheme::Dark.palette()),
    )
    .map(|url| Message::OpenExternal(url.to_string()));
    body = body.push(markdown_body);
    scrollable(container(body).padding([0, 18]).width(Length::Fill))
        .height(Length::Fill)
        .style(theme::scrollable)
        .into()
}

fn install_resource_button(
    project_id: String,
    installing: bool,
) -> iced::widget::Button<'static, Message> {
    button(
        row![
            svg_icon(icons::DOWNLOAD, 16.0),
            text(if installing {
                "Installing..."
            } else {
                "Install"
            })
            .size(13)
            .color(theme::DARK.palette().crust),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .on_press(if installing {
        Message::Noop
    } else {
        Message::InstallModrinthProject(project_id)
    })
    .style(theme::primary_button)
    .padding([8, 12])
}

fn project_icon(icon: Option<&Vec<u8>>, size: f32) -> Element<'_, Message> {
    match icon {
        Some(bytes) => image(image::Handle::from_bytes(bytes.clone()))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .content_fit(iced::ContentFit::Cover)
            .into(),
        None => svg_icon(icons::MODS, size * 0.6),
    }
}

fn badge_text<'a>(label: impl Into<String>) -> Element<'a, Message> {
    container(text(label.into()).size(11))
        .padding([3, 7])
        .style(theme::badge)
        .into()
}

fn format_downloads(downloads: u64) -> String {
    if downloads >= 1_000_000 {
        format!("{:.1}M", downloads as f64 / 1_000_000.0)
    } else if downloads >= 1_000 {
        format!("{:.1}K", downloads as f64 / 1_000.0)
    } else {
        downloads.to_string()
    }
}

fn mod_row<'a>(item: &'a InstalledMod, categories: &'a [String]) -> Element<'a, Message> {
    let id = item.id.clone();
    container(
        row![
            project_icon(item.icon.as_ref(), 34.0),
            checkbox("", item.enabled)
                .on_toggle(move |enabled| Message::ToggleMod {
                    mod_id: id.clone(),
                    enabled
                })
                .style(theme::checkbox),
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
            pick_list(categories.to_vec(), Some(item.category.clone()), {
                let mod_id = item.id.clone();
                move |category| Message::ModCategoryChanged {
                    mod_id: mod_id.clone(),
                    category,
                }
            })
            .style(theme::pick_list)
            .menu_style(theme::pick_list_menu),
            icon_button(
                icons::DELETE,
                16.0,
                Message::DeleteMod(item.id.clone()),
                theme::danger_button
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding(10)
    .style(theme::card)
    .into()
}

fn files<'a>(
    instance: &'a Instance,
    export_path: &'a str,
    export_busy: bool,
) -> Element<'a, Message> {
    column![
        text(instance.path.display().to_string()).size(12),
        row![
            icon_button(
                icons::FOLDER,
                18.0,
                Message::OpenInstanceFiles(instance.id.clone()),
                theme::secondary_button
            ),
            icon_button(
                icons::LOGS,
                18.0,
                Message::OpenInstanceLogs(instance.id.clone()),
                theme::secondary_button
            ),
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
            icon_button(
                icons::FOLDER,
                18.0,
                Message::PickExportZip(instance.id.clone()),
                theme::secondary_button
            ),
            button(if export_busy {
                "Exporting..."
            } else {
                "Export Zip"
            })
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

fn logs<'a>(
    launch_log: &'a [String],
    launch_status: Option<&str>,
    launch_progress: Option<f32>,
) -> Element<'a, Message> {
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
        let mut col = column![text(status_line).size(12)].spacing(4);
        for line in launch_log.iter() {
            col = col.push(log_line(line));
        }
        scrollable(
            container(col)
                .padding(theme::scrollbar_gutter())
                .width(Length::Fill),
        )
        .id(iced::widget::scrollable::Id::new("launch-log-scroll"))
        .height(Length::Fill)
        .style(theme::scrollable)
        .into()
    };

    let mut layout = column![row![
        text("Launch log").size(16),
        Space::with_width(Length::Fill),
        button("Copy log")
            .on_press(Message::CopyLogs)
            .style(theme::secondary_button),
    ]
    .align_y(Alignment::Center),]
    .spacing(10)
    .height(Length::Fill);
    if let Some(progress) = launch_progress {
        layout = layout.push(
            progress_bar(0.0..=1.0, progress)
                .height(Length::Fixed(6.0))
                .style(theme::progress),
        );
    }
    layout = layout.push(
        container(body)
            .padding(12)
            .style(theme::card)
            .width(Length::Fill)
            .height(Length::Fill),
    );
    layout.into()
}

fn log_line(raw: &str) -> Element<'_, Message> {
    let entry = ParsedLogLine::parse(raw);
    let level_color = match entry.level.as_str() {
        "ERROR" => theme::DARK.palette().danger,
        "WARN" => theme::DARK.palette().warning,
        "INFO" => theme::DARK.palette().success,
        _ => theme::DARK.palette().muted,
    };
    container(
        row![
            text(entry.time)
                .size(11)
                .font(Font::MONOSPACE)
                .width(Length::Fixed(82.0)),
            text(entry.thread)
                .size(11)
                .font(Font::MONOSPACE)
                .width(Length::Fixed(112.0)),
            text(entry.level)
                .size(11)
                .font(Font::MONOSPACE)
                .color(level_color)
                .width(Length::Fixed(48.0)),
            text(entry.message)
                .size(11)
                .font(Font::MONOSPACE)
                .color(theme::DARK.palette().text)
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([1, 0])
    .width(Length::Fill)
    .into()
}

struct ParsedLogLine {
    time: String,
    thread: String,
    level: String,
    message: String,
}

impl ParsedLogLine {
    fn parse(raw: &str) -> Self {
        let mut line = raw
            .strip_prefix("stdout: ")
            .or_else(|| raw.strip_prefix("stderr: "))
            .unwrap_or(raw)
            .trim()
            .to_string();
        let fallback_level =
            if raw.starts_with("stderr:") || line.to_ascii_lowercase().contains("error") {
                "ERROR"
            } else if line.to_ascii_lowercase().contains("warn") {
                "WARN"
            } else {
                "INFO"
            };

        if let Some(rest) = line.strip_prefix('[') {
            if let Some((time, rest)) = rest.split_once("] [") {
                if let Some((thread_and_level, message)) = rest.split_once("]:") {
                    let (thread, level) = thread_and_level
                        .rsplit_once('/')
                        .unwrap_or((thread_and_level, fallback_level));
                    return Self {
                        time: format!("{}.000", time.trim()),
                        thread: compact_thread(thread),
                        level: level.trim().to_string(),
                        message: message.trim().to_string(),
                    };
                }
            }
        }

        let level = ["ERROR", "WARN", "INFO", "DEBUG", "TRACE"]
            .into_iter()
            .find(|level| line.contains(level))
            .unwrap_or(fallback_level);
        line = line.replace("stdout:", "").replace("stderr:", "");
        Self {
            time: "--:--:--.---".into(),
            thread: "Launcher".into(),
            level: level.into(),
            message: line.trim().to_string(),
        }
    }
}

fn compact_thread(thread: &str) -> String {
    thread
        .split('/')
        .next()
        .unwrap_or(thread)
        .trim()
        .chars()
        .take(18)
        .collect()
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
        checkbox("Fullscreen", instance.fullscreen)
            .on_toggle(Message::FullscreenChanged)
            .style(theme::checkbox),
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
