use std::collections::HashMap;
use std::path::PathBuf;

use iced::widget::{
    button, column, container, image, markdown, pick_list, progress_bar, row, scrollable, stack,
    text, text_input, Space,
};
use iced::{Alignment, Element, Length, Theme as IcedTheme};

use crate::auth::Session;
use crate::icons::{self, icon_button, icon_button_maybe, svg_icon};
use crate::instances::mods::{ModrinthKind, ModrinthProject, ResourceProvider};
use crate::instances::screenshots::latest_screenshot;
use crate::instances::{Instance, InstanceRunState, InstanceTab, LoaderKind, SortMode};
use crate::messages::{LauncherPage, Message};
use crate::storage::settings::LauncherSettings;
use crate::theme;

fn grid_columns(window_width: f32, list_view: bool) -> usize {
    if list_view {
        1
    } else {
        let cols = (window_width / 240.0) as usize;
        cols.max(1)
    }
}

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

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    session: Option<&'a Session>,
    instances: &'a [Instance],
    avatar_cache: &'a HashMap<String, PathBuf>,
    window_width: f32,
    page: LauncherPage,
    search: &'a str,
    sort: SortMode,
    list_view: bool,
    loader_filter: Option<LoaderKind>,
    settings_open: bool,
    settings: &'a LauncherSettings,
    java_status: &'a str,
    resource_provider: ResourceProvider,
    modrinth_query: &'a str,
    modrinth_kind: ModrinthKind,
    modrinth_results: &'a [ModrinthProject],
    modrinth_detail: Option<&'a crate::instances::mods::ModrinthProjectDetail>,
    modrinth_markdown: &'a [markdown::Item],
    modrinth_detail_busy: bool,
    modrinth_install_status: &'a str,
    modrinth_install_progress: f32,
    modrinth_busy: bool,
    create_open: bool,
    import_open: bool,
    create_versions: &'a [String],
    create_loader_versions: &'a [String],
    create_loader_versions_busy: bool,
    create_busy: bool,
    import_busy: bool,
    create_name: &'a str,
    create_version: &'a str,
    create_loader: LoaderKind,
    create_loader_version: &'a str,
    import_path: &'a str,
    create_status: &'a str,
    create_progress: f32,
    create_paused: bool,
    accounts: &'a [Session],
    account_menu_open: bool,
    error_banner: Option<&'a str>,
    delete_confirm_id: Option<&'a str>,
) -> Element<'a, Message> {
    let username = session.map(|s| s.username.as_str()).unwrap_or("Guest");

    let filtered = filtered_instances(instances, search, sort, loader_filter);
    let content_width = (window_width - 260.0).max(560.0);
    let columns = grid_columns(content_width, list_view);
    let grid: Element<'a, Message> = if instances.is_empty() {
        empty_state(
            "No instances yet",
            "Create your first Minecraft instance to get started.",
            true,
        )
    } else if filtered.is_empty() {
        empty_state(
            "No matches",
            "Try another search term or clear the search box.",
            false,
        )
    } else {
        let mut rows = column![].spacing(14);
        for chunk in filtered.chunks(columns) {
            let mut line = row![].spacing(14).width(Length::Fill);
            for instance in chunk {
                line = line.push(card(instance, list_view, columns));
            }
            if chunk.len() < columns && !list_view {
                for _ in chunk.len()..columns {
                    line = line.push(Space::with_width(Length::FillPortion(1)));
                }
            }
            rows = rows.push(line);
        }
        scrollable(rows).height(Length::Fill).into()
    };

    let active_instance = filtered.first().copied().or_else(|| instances.first());
    let topbar = topbar(search, active_instance, session, avatar_cache, username);
    let sidebar = sidebar(username, session, avatar_cache, page);
    let mut content = column![topbar].spacing(18).padding([18, 24]);
    if let Some(error) = error_banner {
        content = content.push(error_row(error));
    }
    content = match page {
        LauncherPage::Home => content.push(home_dashboard(active_instance, instances)),
        LauncherPage::Instances => content.push(instances_page(sort, list_view, loader_filter, grid)),
        LauncherPage::Downloads => content.push(downloads_page()),
        LauncherPage::Discover => content.push(discover_page(
            resource_provider,
            modrinth_query,
            modrinth_kind,
            modrinth_results,
            modrinth_detail,
            modrinth_markdown,
            modrinth_detail_busy,
            modrinth_install_status,
            modrinth_install_progress,
            modrinth_busy,
        )),
        LauncherPage::Accounts => content.push(account_switcher(accounts, session)),
        LauncherPage::Settings => content.push(crate::screens::settings::view(
            settings,
            java_status,
            accounts,
            session,
        )),
    };
    if account_menu_open {
        content = content.push(account_switcher(accounts, session));
    }

    let shell = row![
        sidebar,
        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(theme::app_container),
    ]
    .height(Length::Fill)
    .width(Length::Fill);

    let mut base: Element<'a, Message> = container(shell)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(theme::app_container)
        .into();

    if settings_open {
        base = stack![
            base,
            crate::screens::settings::view(settings, java_status, accounts, session)
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }

    if modrinth_detail.is_some() || modrinth_detail_busy {
        base = stack![
            base,
            resource_detail_overlay(
                modrinth_detail,
                modrinth_markdown,
                modrinth_detail_busy,
                modrinth_install_status,
                modrinth_install_progress,
            )
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }

    if create_open || import_open {
        let overlay = if create_open {
            create_instance_overlay(
                create_versions,
                create_loader_versions,
                create_loader_versions_busy,
                create_busy,
                create_name,
                create_version,
                create_loader,
                create_loader_version,
                create_status,
                create_progress,
                create_paused,
            )
        } else {
            import_instance_overlay(import_path, import_busy)
        };
        stack![base, overlay]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else if let Some(id) = delete_confirm_id {
        let name = instances
            .iter()
            .find(|instance| instance.id == id)
            .map(|instance| instance.name.as_str())
            .unwrap_or("this instance");
        stack![base, delete_confirm_overlay(id, name)]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        base
    }
}

fn sidebar<'a>(
    username: &'a str,
    session: Option<&'a Session>,
    avatar_cache: &'a HashMap<String, PathBuf>,
    page: LauncherPage,
) -> Element<'a, Message> {
    let nav = column![
        sidebar_item(icons::HOME, "Home", page == LauncherPage::Home, Message::LauncherPageSelected(LauncherPage::Home)),
        sidebar_item(icons::GRID_VIEW, "Instances", page == LauncherPage::Instances, Message::LauncherPageSelected(LauncherPage::Instances)),
        sidebar_item(icons::MODS, "Discover", page == LauncherPage::Discover, Message::LauncherPageSelected(LauncherPage::Discover)),
        sidebar_item(icons::ACCOUNT, "Accounts", page == LauncherPage::Accounts, Message::LauncherPageSelected(LauncherPage::Accounts)),
        sidebar_item(icons::DOWNLOAD, "Downloads", page == LauncherPage::Downloads, Message::LauncherPageSelected(LauncherPage::Downloads)),
        sidebar_item(
            icons::SETTINGS,
            "Settings",
            false,
            Message::LauncherPageSelected(LauncherPage::Settings)
        ),
    ]
    .spacing(8);

    container(
        column![
            column![
                text("Swift Launcher")
                    .size(22)
                    .color(theme::DARK.palette().accent),
                text(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .size(12)
                    .color(theme::DARK.palette().muted),
            ]
            .spacing(6),
            nav,
            Space::with_height(Length::Fill),
            account_chip(session, avatar_cache, username),
        ]
        .spacing(28),
    )
    .width(Length::Fixed(240.0))
    .height(Length::Fill)
    .padding([28, 14])
    .style(theme::sidebar)
    .into()
}

fn sidebar_item(
    icon: &'static [u8],
    label: &'static str,
    active: bool,
    message: Message,
) -> Element<'static, Message> {
    let style = if active {
        theme::nav_button
    } else {
        theme::ghost_button
    };
    button(
        row![svg_icon(icon, 18.0), text(label).size(15)]
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .on_press(message)
    .style(style)
    .padding([10, 12])
    .width(Length::Fill)
    .into()
}

fn topbar<'a>(
    search: &'a str,
    active_instance: Option<&'a Instance>,
    session: Option<&'a Session>,
    avatar_cache: &'a HashMap<String, PathBuf>,
    username: &'a str,
) -> Element<'a, Message> {
    let launch_message = active_instance
        .map(|instance| match instance.run_state {
            InstanceRunState::Running => Message::StopInstance(instance.id.clone()),
            InstanceRunState::Preparing => Message::Noop,
            InstanceRunState::Idle => Message::PlayInstance(instance.id.clone()),
        })
        .unwrap_or(Message::Noop);

    container(
        row![
            text_input("Search instances, mods, accounts...", search)
                .on_input(Message::SearchChanged)
                .style(theme::input)
                .padding(10)
                .width(Length::Fixed(420.0)),
            Space::with_width(Length::Fill),
            icon_button(icons::ALERT, 18.0, Message::Noop, theme::ghost_button),
            icon_button(icons::LOGS, 18.0, Message::Noop, theme::ghost_button),
            account_chip(session, avatar_cache, username),
            button(
                row![svg_icon(icons::PLAY, 18.0), text("Launch Game").size(15)]
                    .spacing(10)
                    .align_y(Alignment::Center),
            )
            .on_press(launch_message)
            .style(theme::primary_button)
            .padding([10, 18]),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .height(Length::Fixed(64.0))
    .width(Length::Fill)
    .into()
}

fn home_dashboard<'a>(
    active: Option<&'a Instance>,
    instances: &'a [Instance],
) -> Element<'a, Message> {
    let hero = match active {
        Some(instance) => active_instance_hero(instance),
        None => empty_hero(),
    };
    let recent = recent_instances_panel(instances);
    let upper = row![hero, recent]
        .spacing(18)
        .height(Length::Fixed(330.0));
    let middle = row![
        system_panel(instances.len()),
        up_next_panel(instances),
        weekly_picks_panel(),
    ]
    .spacing(18)
    .height(Length::Fixed(230.0));

    scrollable(
        column![upper, middle, recent_activity_panel(instances)]
            .spacing(18)
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

fn instances_page<'a>(
    sort: SortMode,
    list_view: bool,
    loader_filter: Option<LoaderKind>,
    grid: Element<'a, Message>,
) -> Element<'a, Message> {
    column![section_header(sort, list_view, loader_filter), grid]
        .spacing(16)
        .height(Length::Fill)
        .into()
}

fn downloads_page<'a>() -> Element<'a, Message> {
    container(
        column![
            text("Queue Management").size(30),
            text("Downloads surface is ready for active queue wiring.")
                .size(13)
                .color(theme::DARK.palette().muted),
            container(
                column![
                    text("No active downloads").size(18),
                    text("Install or launch an instance to see live asset progress here.")
                        .size(13)
                        .color(theme::DARK.palette().muted),
                ]
                .spacing(8),
            )
            .padding(18)
            .style(theme::card)
            .width(Length::Fill),
        ]
        .spacing(16),
    )
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}

#[allow(clippy::too_many_arguments)]
fn discover_page<'a>(
    provider: ResourceProvider,
    query: &'a str,
    kind: ModrinthKind,
    results: &'a [ModrinthProject],
    _detail: Option<&'a crate::instances::mods::ModrinthProjectDetail>,
    _markdown: &'a [markdown::Item],
    _detail_busy: bool,
    _install_status: &'a str,
    _install_progress: f32,
    busy: bool,
) -> Element<'a, Message> {
    let mut result_list = column![].spacing(12);
    if busy {
        result_list = result_list.push(
            container(text("Searching resources...").size(14))
                .padding(16)
                .style(theme::card),
        );
    } else if results.is_empty() {
        result_list = result_list.push(
            container(
                column![
                    text("No resources loaded").size(18),
                    text("Choose a type, search by name, then install or open a project page.")
                        .size(13)
                        .color(theme::DARK.palette().muted),
                ]
                .spacing(8),
            )
            .padding(18)
            .style(theme::card),
        );
    } else {
        for item in results {
            result_list = result_list.push(discover_result_row(item));
        }
    }

    container(
        column![
            row![
                text(provider.to_string())
                    .size(28)
                    .color(theme::DARK.palette().accent),
                Space::with_width(Length::Fill),
                styled_pick_list(
                    ResourceProvider::ALL,
                    Some(provider),
                    Message::ResourceProviderSelected
                )
                .width(Length::Fixed(160.0)),
            ]
            .align_y(Alignment::Center),
            row![
                discover_kind_tabs(kind),
                column![
                    row![
                        text_input("Search mods...", query)
                            .on_input(Message::ModrinthSearchChanged)
                            .on_submit(Message::SearchModrinth)
                            .style(theme::input)
                            .padding(10),
                        button(if busy { "Searching..." } else { "Search" })
                            .on_press(if busy {
                                Message::Noop
                            } else {
                                Message::SearchModrinth
                            })
                            .style(theme::primary_button)
                            .padding([10, 16]),
                    ]
                    .spacing(10),
                    scrollable(result_list).height(Length::Fill),
                ]
                .spacing(14)
                .width(Length::Fill),
            ]
            .spacing(14)
            .height(Length::Fill),
        ]
        .spacing(16),
    )
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}

fn resource_detail_overlay<'a>(
    detail: Option<&'a crate::instances::mods::ModrinthProjectDetail>,
    parsed_markdown: &'a [markdown::Item],
    busy: bool,
    install_status: &'a str,
    install_progress: f32,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = if busy {
        container(text("Loading project page...").size(16))
            .padding(18)
            .style(theme::card)
            .into()
    } else if let Some(detail) = detail {
        let markdown_body = markdown::view(
            parsed_markdown,
            markdown::Settings::with_text_size(13),
            markdown::Style::from_palette(IcedTheme::Dark.palette()),
        )
        .map(|url| Message::OpenExternal(url.to_string()));
        let install_status = if install_status.trim().is_empty() {
            "Ready to install"
        } else {
            install_status
        };
        container(
            column![
                row![
                    project_icon(detail.icon.as_ref(), 58.0),
                    column![
                        text(&detail.title).size(22),
                        row![
                            badge(detail.kind.to_string()),
                            badge(format!("{} Downloads", format_downloads(detail.downloads))),
                        ]
                        .spacing(6),
                        text(&detail.description)
                            .size(13)
                            .color(theme::DARK.palette().muted),
                    ]
                    .spacing(6),
                    Space::with_width(Length::Fill),
                    icon_button(
                        icons::CLOSE,
                        18.0,
                        Message::CloseModrinthProject,
                        theme::secondary_button
                    ),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                row![
                    button(row![svg_icon(icons::DOWNLOAD, 15.0), text("Install").size(14)])
                        .on_press(Message::InstallModrinthProject(detail.project_id.clone()))
                        .style(theme::primary_button)
                        .padding([8, 14]),
                    text(install_status).size(12).color(theme::DARK.palette().muted),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                progress_bar(0.0..=1.0, install_progress)
                    .height(Length::Fixed(6.0))
                    .style(theme::progress),
                scrollable(markdown_body).height(Length::Fill),
            ]
            .spacing(14),
        )
        .padding(18)
        .style(theme::shell)
        .into()
    } else {
        container(text("Project page unavailable").size(14))
            .padding(18)
            .style(theme::card)
            .into()
    };

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(36)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::scrim)
        .into()
}

fn discover_kind_tabs(selected: ModrinthKind) -> Element<'static, Message> {
    let items = [
        (ModrinthKind::Mods, "Mods"),
        (ModrinthKind::Modpacks, "Modpacks"),
        (ModrinthKind::ResourcePacks, "Resourcepacks"),
        (ModrinthKind::Shaders, "Shaders"),
    ];
    let mut tabs = column![].spacing(0);
    for (kind, label) in items {
        let style = if kind == selected {
            theme::nav_button
        } else {
            theme::ghost_button
        };
        tabs = tabs.push(
            button(text(label).size(15))
                .on_press(Message::ModrinthKindSelected(kind))
                .style(style)
                .padding([9, 14])
                .width(Length::Fill),
        );
    }
    container(tabs)
        .width(Length::Fixed(150.0))
        .style(theme::card)
        .into()
}

fn discover_result_row(item: &ModrinthProject) -> Element<'_, Message> {
    container(
        row![
            project_icon(item.icon.as_ref(), 64.0),
            column![
                row![
                    text(&item.title).size(18),
                    text(format!("by {}", item.author))
                        .size(13)
                        .color(theme::DARK.palette().muted),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
                text(&item.description)
                    .size(14)
                    .width(Length::Fill)
                    .color(theme::DARK.palette().text),
                discover_meta(item),
            ]
            .spacing(8)
            .width(Length::Fill),
            column![
                text(format!("{} Downloads", format_downloads(item.downloads)))
                    .size(15)
                    .color(theme::DARK.palette().text),
                button(
                    container(
                        row![svg_icon(icons::DOWNLOAD, 15.0), text("Install").size(14)]
                            .spacing(8)
                            .align_y(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .center_x(Length::Fill),
                )
                    .on_press(Message::InstallModrinthProject(item.project_id.clone()))
                    .style(theme::primary_button)
                    .padding([8, 14])
                    .width(Length::Fixed(160.0)),
                button(
                    container(text("Open Page").size(14))
                        .width(Length::Fill)
                        .center_x(Length::Fill),
                )
                    .on_press(Message::OpenModrinthProject(item.project_id.clone()))
                    .style(theme::secondary_button)
                    .padding([8, 14])
                    .width(Length::Fixed(160.0)),
            ]
            .spacing(8)
            .align_x(Alignment::End),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .padding(16)
    .width(Length::Fill)
    .style(theme::card)
    .into()
}

fn discover_meta(item: &ModrinthProject) -> Element<'_, Message> {
    let side = match (item.client_side.as_deref(), item.server_side.as_deref()) {
        (Some("required"), Some("required")) => "Client or server".to_string(),
        (Some("required"), _) => "Client only".to_string(),
        (_, Some("required")) => "Server only".to_string(),
        _ => match item.kind {
            ModrinthKind::ResourcePacks | ModrinthKind::Shaders => "Client only".to_string(),
            _ => "Client or server".to_string(),
        },
    };
    let mut meta = row![badge(side)].spacing(6).align_y(Alignment::Center);
    let loaders = if item.loaders.is_empty() {
        vec![item.provider.to_string()]
    } else {
        item.loaders.clone()
    };
    for loader in loaders.into_iter().take(3) {
        meta = meta.push(badge(title_case(&loader)));
    }
    for category in item.categories.iter().take(3) {
        meta = meta.push(badge(title_case(category)));
    }
    meta.into()
}

fn project_icon(icon: Option<&Vec<u8>>, size: f32) -> Element<'_, Message> {
    match icon {
        Some(bytes) => image(image::Handle::from_bytes(bytes.clone()))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .content_fit(iced::ContentFit::Cover)
            .into(),
        None => container(svg_icon(icons::MODS, size * 0.55))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(theme::surface)
            .into(),
    }
}

fn title_case(value: &str) -> String {
    value
        .split(['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn format_downloads(downloads: u64) -> String {
    if downloads >= 1_000_000 {
        format!("{:.2}M", downloads as f64 / 1_000_000.0)
    } else if downloads >= 1_000 {
        format!("{:.1}K", downloads as f64 / 1_000.0)
    } else {
        downloads.to_string()
    }
}

fn active_instance_hero(instance: &Instance) -> Element<'_, Message> {
    container(
        column![
            card_artwork(instance, 96.0),
            row![
                badge("Active Instance"),
                badge(&instance.minecraft_version),
                badge(loader_label(instance.loader)),
            ]
            .spacing(8),
            Space::with_height(Length::Fill),
            text(&instance.name).size(30),
            text(format!(
                "{} instance, {} RAM allocated. Last played {}.",
                loader_label(instance.loader),
                format_ram(instance.ram_mb),
                instance
                    .last_played_unix
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "never".into())
            ))
            .size(13)
            .color(theme::DARK.palette().muted),
            row![
                button(row![svg_icon(icons::PLAY, 18.0), text("Play Now").size(16)])
                    .on_press(match instance.run_state {
                        InstanceRunState::Running => Message::StopInstance(instance.id.clone()),
                        InstanceRunState::Preparing => Message::Noop,
                        InstanceRunState::Idle => Message::PlayInstance(instance.id.clone()),
                    })
                    .style(theme::primary_button)
                    .padding([12, 18]),
                button("Settings")
                    .on_press(Message::OpenInstanceTab(
                        instance.id.clone(),
                        InstanceTab::Settings
                    ))
                    .style(theme::secondary_button)
                    .padding([12, 18]),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .spacing(10),
    )
    .padding(24)
    .width(Length::FillPortion(3))
    .height(Length::Fill)
    .style(theme::shell)
    .into()
}

fn empty_hero<'a>() -> Element<'a, Message> {
    container(
        column![
            svg_icon(icons::CREEPER, 54.0),
            text("No active instance").size(26),
            text("Create or import an instance to start playing.")
                .size(13)
                .color(theme::DARK.palette().muted),
            button("Create Instance")
                .on_press(Message::NewInstance)
                .style(theme::primary_button)
                .padding([10, 18]),
        ]
        .spacing(12)
        .align_x(Alignment::Center),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .width(Length::FillPortion(3))
    .height(Length::Fill)
    .style(theme::shell)
    .into()
}

fn system_panel(instance_count: usize) -> Element<'static, Message> {
    container(
        column![
            text("System Status")
                .size(18)
                .color(theme::DARK.palette().accent),
            text("Launcher telemetry").size(12).color(theme::DARK.palette().muted),
            metric_bar("RAM Allocation", "4 GB / 16 GB", 0.25, theme::DARK.palette().warning),
            metric_bar("Disk Storage", "64%", 0.64, theme::DARK.palette().accent),
            Space::with_height(Length::Fill),
            row![
                text("Instances").size(12).color(theme::DARK.palette().muted),
                Space::with_width(Length::Fill),
                text(instance_count.to_string()).size(14),
            ],
            row![
                text("Systems").size(12).color(theme::DARK.palette().muted),
                Space::with_width(Length::Fill),
                text("Online").size(14).color(theme::DARK.palette().success),
            ],
        ]
        .spacing(12),
    )
    .padding(18)
    .width(Length::FillPortion(1))
    .height(Length::Fill)
    .style(theme::card)
    .into()
}

fn recent_instances_panel<'a>(instances: &'a [Instance]) -> Element<'a, Message> {
    let mut list = column![
        text("Recent Instances").size(18),
        text("Fast resume").size(12).color(theme::DARK.palette().muted),
    ]
    .spacing(10);

    for instance in recent_instances(instances).into_iter().take(3) {
        list = list.push(
            button(
                row![
                    card_artwork(instance, 44.0),
                    column![
                        text(&instance.name).size(14),
                        text(format!(
                            "{} • {}",
                            instance.minecraft_version,
                            loader_label(instance.loader)
                        ))
                        .size(11)
                        .color(theme::DARK.palette().muted),
                    ]
                    .spacing(2),
                    Space::with_width(Length::Fill),
                    svg_icon(icons::PLAY, 15.0),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .on_press(Message::PlayInstance(instance.id.clone()))
            .style(theme::ghost_button)
            .padding(8),
        );
    }

    if instances.is_empty() {
        list = list.push(text("No instances yet").size(13));
    }

    container(list)
        .padding(18)
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .style(theme::card)
        .into()
}

fn up_next_panel<'a>(instances: &'a [Instance]) -> Element<'a, Message> {
    let mut list = column![row![
        text("Up Next").size(18),
        Space::with_width(Length::Fill),
        text("UPDATE ALL")
            .size(11)
            .color(theme::DARK.palette().accent),
    ]]
    .spacing(10);

    for instance in instances.iter().take(3) {
        list = list.push(
            row![
                svg_icon(icons::DOWNLOAD, 18.0),
                column![
                    text(&instance.name).size(13),
                    text(format!("{} assets ready", instance.minecraft_version))
                        .size(11)
                        .color(theme::DARK.palette().muted),
                ]
                .spacing(2),
                Space::with_width(Length::Fill),
                icon_button(
                    icons::PLAY,
                    15.0,
                    Message::PlayInstance(instance.id.clone()),
                    theme::ghost_button
                ),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
    }

    if instances.is_empty() {
        list = list.push(text("No queued updates").size(13));
    }

    container(list)
        .padding(18)
        .width(Length::FillPortion(2))
        .height(Length::Fill)
        .style(theme::card)
        .into()
}

fn weekly_picks_panel<'a>() -> Element<'a, Message> {
    container(
        column![
            Space::with_height(Length::Fill),
            text("Weekly Picks").size(18),
            text("Discover utility mods curated for high-performance instances.")
                .size(12)
                .color(theme::DARK.palette().muted),
            button("Explore Now")
                .on_press(Message::LauncherPageSelected(LauncherPage::Discover))
                .style(theme::secondary_button)
                .padding([8, 12])
                .width(Length::Fill),
        ]
        .spacing(10),
    )
    .padding(18)
    .width(Length::FillPortion(1))
    .height(Length::Fill)
    .style(theme::card)
    .into()
}

fn recent_activity_panel<'a>(instances: &'a [Instance]) -> Element<'a, Message> {
    let mut cards = row![].spacing(14).width(Length::Fill);
    for instance in recent_instances(instances).into_iter().take(4) {
        cards = cards.push(
            container(
                column![
                    text("Played Instance")
                        .size(11)
                        .color(theme::DARK.palette().accent),
                    text(&instance.name).size(14),
                    text(format!(
                        "{} min session",
                        (instance.playtime_seconds / 60).max(1)
                    ))
                    .size(12)
                    .color(theme::DARK.palette().muted),
                ]
                .spacing(8),
            )
            .padding(14)
            .style(theme::surface)
            .width(Length::FillPortion(1)),
        );
    }

    if instances.is_empty() {
        cards = cards.push(
            container(text("No recent activity").size(13))
                .padding(14)
                .style(theme::surface)
                .width(Length::Fill),
        );
    }

    container(column![text("Recent Activity").size(18), cards].spacing(14))
        .padding(18)
        .style(theme::card)
        .width(Length::Fill)
        .into()
}

fn recent_instances(instances: &[Instance]) -> Vec<&Instance> {
    let mut recent = instances.iter().collect::<Vec<_>>();
    recent.sort_by_key(|instance| std::cmp::Reverse(instance.last_played_unix.unwrap_or(0)));
    recent
}

fn metric_bar(
    label: &'static str,
    value: &'static str,
    progress: f32,
    color: iced::Color,
) -> Element<'static, Message> {
    let _ = color;
    column![
        row![
            text(label).size(11).color(theme::DARK.palette().muted),
            Space::with_width(Length::Fill),
            text(value).size(11).color(theme::DARK.palette().accent),
        ],
        progress_bar(0.0..=1.0, progress)
            .height(Length::Fixed(7.0))
            .style(theme::progress),
    ]
    .spacing(6)
    .into()
}

fn section_header(
    sort: SortMode,
    list_view: bool,
    loader_filter: Option<LoaderKind>,
) -> Element<'static, Message> {
    column![
        row![
            column![
                text("Instance Library")
                    .size(28)
                    .color(theme::DARK.palette().accent),
                text("Manage profiles, loaders, files, and launch logs.")
                    .size(13)
                    .color(theme::DARK.palette().muted),
            ]
            .spacing(4),
            Space::with_width(Length::Fill),
            icon_button(icons::ADD, 18.0, Message::NewInstance, theme::primary_button),
            icon_button(
                icons::IMPORT,
                18.0,
                Message::ImportInstance,
                theme::secondary_button
            ),
            styled_pick_list(
                [SortMode::Name, SortMode::LastPlayed, SortMode::Version],
                Some(sort),
                Message::SortChanged
            ),
            icon_button(
                if list_view {
                    icons::GRID_VIEW
                } else {
                    icons::LIST_VIEW
                },
                18.0,
                Message::ToggleListView(!list_view),
                theme::secondary_button,
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
        loader_filter_badges(loader_filter),
    ]
    .spacing(10)
    .into()
}

fn loader_filter_badges(selected: Option<LoaderKind>) -> Element<'static, Message> {
    let loaders = [
        LoaderKind::Vanilla,
        LoaderKind::Fabric,
        LoaderKind::Forge,
        LoaderKind::NeoForge,
        LoaderKind::Quilt,
    ];
    let mut row = row![].spacing(8).align_y(Alignment::Center);
    for loader in loaders {
        let style = if selected == Some(loader) {
            theme::nav_button
        } else {
            theme::secondary_button
        };
        row = row.push(
            button(loader_label(loader))
                .on_press(Message::InstanceLoaderFilterChanged(if selected == Some(loader) {
                    None
                } else {
                    Some(loader)
                }))
                .style(style)
                .padding([6, 14]),
        );
    }
    row.into()
}

fn format_ram(ram_mb: u32) -> String {
    if ram_mb >= 1024 {
        format!("{} GB", ram_mb / 1024)
    } else {
        format!("{ram_mb} MB")
    }
}

fn empty_state<'a>(title: &'a str, subtitle: &'a str, show_create: bool) -> Element<'a, Message> {
    let mut body = column![
        svg_icon(icons::CREEPER, 56.0),
        text(title).size(24),
        text(subtitle).size(13),
    ]
    .spacing(14)
    .align_x(Alignment::Center);
    if show_create {
        body = body.push(icon_button(
            icons::ADD,
            18.0,
            Message::NewInstance,
            theme::primary_button,
        ));
    }
    container(body)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
}

fn account_chip<'a>(
    session: Option<&'a Session>,
    avatar_cache: &'a HashMap<String, PathBuf>,
    username: &'a str,
) -> Element<'a, Message> {
    let avatar = session.and_then(|s| {
        avatar_cache.get(&s.uuid).map(|path| {
            image(image::Handle::from_path(path))
                .width(28.0)
                .height(28.0)
                .into()
        })
    });
    let leading: Element<'a, Message> =
        avatar.unwrap_or_else(|| icons::avatar_placeholder(username, 36.0));
    button(
        row![leading, text(username).size(13)]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .on_press(Message::AccountMenuToggled)
    .style(theme::secondary_button)
    .padding([6, 10])
    .into()
}

fn error_row(error: &str) -> Element<'_, Message> {
    container(
        row![
            svg_icon(icons::ALERT, 18.0),
            text(error).size(13).width(Length::Fill),
            icon_button(
                icons::CLOSE,
                16.0,
                Message::ErrorDismissed,
                theme::danger_button
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding(10)
    .style(theme::banner)
    .width(Length::Fill)
    .into()
}

fn account_switcher<'a>(
    accounts: &'a [Session],
    active: Option<&'a Session>,
) -> Element<'a, Message> {
    let mut cards = column![].spacing(12);

    if accounts.is_empty() {
        cards = cards.push(
            container(
                column![
                    svg_icon(icons::ACCOUNT, 42.0),
                    text("No saved accounts").size(20),
                    text("Add a Microsoft, Ely.by, or LittleSkin profile to start playing.")
                        .size(13)
                        .color(theme::DARK.palette().muted),
                    button("Add Account")
                        .on_press(Message::AddAccount)
                        .style(theme::primary_button)
                        .padding([10, 16]),
                ]
                .spacing(12)
                .align_x(Alignment::Center),
            )
            .padding(28)
            .style(theme::card)
            .width(Length::Fill),
        );
    }

    for account in accounts {
        let is_active = active.is_some_and(|session| session.uuid == account.uuid);
        let initials = account
            .username
            .chars()
            .take(2)
            .collect::<String>()
            .to_uppercase();
        cards = cards.push(
            container(row![
                container(text(initials).size(18).color(theme::DARK.palette().accent))
                    .width(Length::Fixed(54.0))
                    .height(Length::Fixed(54.0))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .style(theme::surface),
                column![
                    row![
                        text(&account.username).size(18),
                        if is_active {
                            badge("ACTIVE")
                        } else {
                            badge("SAVED")
                        },
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(format!("{} profile", account.provider))
                        .size(13)
                        .color(theme::DARK.palette().muted),
                    text(&account.uuid).size(11).color(theme::DARK.palette().muted),
                ]
                .spacing(5),
                Space::with_width(Length::Fill),
                button(if is_active { "Current" } else { "Use Account" })
                    .on_press(if is_active {
                        Message::Noop
                    } else {
                        Message::AccountSelected(account.uuid.clone())
                    })
                    .style(if is_active {
                        theme::nav_button
                    } else {
                        theme::secondary_button
                    })
                    .padding([8, 12]),
                button("Sign out")
                    .on_press(Message::SignOut(account.uuid.clone()))
                    .style(theme::danger_button)
                    .padding([8, 12]),
            ]
            .spacing(14)
            .align_y(Alignment::Center))
            .padding(14)
            .style(theme::card),
        );
    }

    let summary = container(
        row![
            column![
                text("Accounts").size(30).color(theme::DARK.palette().accent),
                text("Manage launcher identities and switch active sessions.")
                    .size(13)
                    .color(theme::DARK.palette().muted),
            ]
            .spacing(4),
            Space::with_width(Length::Fill),
            button("Add Account")
                .on_press(Message::AddAccount)
                .style(theme::primary_button)
                .padding([10, 16]),
        ]
        .align_y(Alignment::Center),
    )
    .padding(18)
    .style(theme::shell)
    .width(Length::Fill);

    column![summary, cards]
        .spacing(16)
        .height(Length::Fill)
        .into()
}

fn filtered_instances<'a>(
    instances: &'a [Instance],
    search: &str,
    sort: SortMode,
    loader_filter: Option<LoaderKind>,
) -> Vec<&'a Instance> {
    let mut filtered: Vec<_> = instances
        .iter()
        .filter(|instance| {
            instance
                .name
                .to_lowercase()
                .contains(&search.to_lowercase())
                && loader_filter.is_none_or(|loader| instance.loader == loader)
        })
        .collect();
    match sort {
        SortMode::Name => filtered.sort_by_key(|instance| instance.name.clone()),
        SortMode::LastPlayed => {
            filtered.sort_by_key(|instance| std::cmp::Reverse(instance.last_played_unix))
        }
        SortMode::Version => filtered.sort_by(|a, b| a.minecraft_version.cmp(&b.minecraft_version)),
    }
    filtered
}

fn play_button(instance: &Instance) -> Element<'_, Message> {
    match instance.run_state {
        InstanceRunState::Running => icon_button_maybe(
            icons::STOP,
            18.0,
            Some(Message::StopInstance(instance.id.clone())),
            theme::danger_button,
        )
        .into(),
        InstanceRunState::Preparing => {
            icon_button_maybe(icons::PLAY, 18.0, None, theme::secondary_button).into()
        }
        InstanceRunState::Idle => icon_button_maybe(
            icons::PLAY,
            18.0,
            Some(Message::PlayInstance(instance.id.clone())),
            theme::success_button,
        )
        .into(),
    }
}

fn card(instance: &Instance, list_view: bool, _columns: usize) -> Element<'_, Message> {
    if list_view {
        container(
            row![
                card_artwork(instance, 40.0),
                column![
                    text(&instance.name).size(14),
                    text(format!(
                        "{} • {}",
                        instance.minecraft_version,
                        loader_label(instance.loader)
                    ))
                    .size(11),
                    loader_badge(instance.loader),
                    run_state_label(instance.run_state),
                ]
                .spacing(2)
                .width(Length::FillPortion(3)),
                play_button(instance),
                icon_button(
                    icons::SETTINGS,
                    18.0,
                    Message::OpenInstanceTab(instance.id.clone(), InstanceTab::Settings),
                    theme::secondary_button,
                ),
                icon_button(
                    icons::LOGS,
                    18.0,
                    Message::OpenInstanceTab(instance.id.clone(), InstanceTab::Logs),
                    theme::secondary_button,
                ),
                icon_button(
                    icons::DELETE,
                    18.0,
                    Message::RequestDeleteInstance(instance.id.clone()),
                    theme::danger_button,
                ),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(10)
        .style(theme::card)
        .into()
    } else {
        container(
            column![
                card_artwork(instance, 110.0),
                container(text(&instance.name).size(13))
                    .width(Length::Fill)
                    .center_x(Length::Fill),
                row![
                    badge(&instance.minecraft_version),
                    loader_badge(instance.loader),
                    run_state_label(instance.run_state),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
                row![
                    play_button(instance),
                    icon_button(
                        icons::SETTINGS,
                        18.0,
                        Message::OpenInstanceTab(instance.id.clone(), InstanceTab::Settings),
                        theme::secondary_button,
                    ),
                    icon_button(
                        icons::LOGS,
                        18.0,
                        Message::OpenInstanceTab(instance.id.clone(), InstanceTab::Logs),
                        theme::secondary_button,
                    ),
                    icon_button(
                        icons::DELETE,
                        18.0,
                        Message::RequestDeleteInstance(instance.id.clone()),
                        theme::danger_button,
                    ),
                ]
                .spacing(4),
            ]
            .spacing(6)
            .align_x(Alignment::Center),
        )
        .width(Length::FillPortion(1))
        .height(Length::Shrink)
        .padding(10)
        .style(theme::card)
        .into()
    }
}

fn card_artwork(instance: &Instance, height: f32) -> Element<'_, Message> {
    let screenshot = instance
        .artwork_path
        .as_ref()
        .filter(|path| path.exists())
        .cloned()
        .or_else(|| latest_screenshot(&instance.path));

    if let Some(path) = screenshot {
        container(
            image(image::Handle::from_path(path))
                .width(Length::Fill)
                .height(height)
                .content_fit(iced::ContentFit::Cover),
        )
        .height(height)
        .clip(true)
        .style(theme::surface)
        .into()
    } else {
        container(svg_icon(icons::CREEPER, height * 0.55))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .height(height)
            .style(theme::surface)
            .into()
    }
}

fn run_state_label(state: InstanceRunState) -> Element<'static, Message> {
    let (label, color) = match state {
        InstanceRunState::Idle => ("Idle", theme::DARK.palette().muted),
        InstanceRunState::Preparing => ("Launching", theme::DARK.palette().warning),
        InstanceRunState::Running => ("Running", theme::DARK.palette().success),
    };
    container(text(label).size(11).color(color))
        .padding([3, 7])
        .style(theme::badge)
        .into()
}

fn badge<'a>(label: impl Into<String>) -> Element<'a, Message> {
    container(text(label.into()).size(11))
        .padding([3, 7])
        .style(theme::badge)
        .into()
}

fn loader_badge(loader: LoaderKind) -> Element<'static, Message> {
    let color = match loader {
        LoaderKind::Vanilla => theme::DARK.palette().muted,
        LoaderKind::Fabric => theme::DARK.palette().success,
        LoaderKind::Forge => theme::DARK.palette().warning,
        LoaderKind::NeoForge => theme::DARK.palette().accent,
        LoaderKind::Quilt => theme::DARK.palette().danger,
    };
    container(text(loader_label(loader)).size(11).color(color))
        .padding([3, 9])
        .style(theme::badge)
        .into()
}

fn loader_label(loader: LoaderKind) -> &'static str {
    match loader {
        LoaderKind::Vanilla => "Vanilla",
        LoaderKind::Fabric => "Fabric",
        LoaderKind::Forge => "Forge",
        LoaderKind::NeoForge => "NeoForge",
        LoaderKind::Quilt => "Quilt",
    }
}

#[allow(clippy::too_many_arguments)]
fn create_instance_overlay<'a>(
    versions: &'a [String],
    loader_versions: &'a [String],
    loader_versions_busy: bool,
    busy: bool,
    name: &'a str,
    selected_version: &'a str,
    loader: LoaderKind,
    selected_loader_version: &'a str,
    status: &'a str,
    progress: f32,
    paused: bool,
) -> Element<'a, Message> {
    let version_pick = if versions.is_empty() {
        styled_pick_list(
            Vec::<String>::new(),
            None::<String>,
            Message::CreateInstanceVersionChanged,
        )
        .placeholder("Loading versions...")
        .width(Length::Fill)
    } else {
        styled_pick_list(
            versions.to_vec(),
            Some(selected_version.to_string()),
            Message::CreateInstanceVersionChanged,
        )
        .placeholder("Minecraft version")
        .width(Length::Fill)
    };

    let loader_version_pick: Element<'a, Message> = if loader == LoaderKind::Vanilla {
        text("Vanilla uses the selected Minecraft version directly.")
            .size(12)
            .into()
    } else if loader_versions_busy {
        styled_pick_list(
            Vec::<String>::new(),
            None::<String>,
            Message::CreateInstanceLoaderVersionChanged,
        )
        .placeholder("Loading loader versions...")
        .width(Length::Fill)
        .into()
    } else if loader_versions.is_empty() {
        styled_pick_list(
            Vec::<String>::new(),
            None::<String>,
            Message::CreateInstanceLoaderVersionChanged,
        )
        .placeholder("No loader versions loaded")
        .width(Length::Fill)
        .into()
    } else {
        styled_pick_list(
            loader_versions.to_vec(),
            Some(selected_loader_version.to_string()),
            Message::CreateInstanceLoaderVersionChanged,
        )
        .placeholder("Loader version")
        .width(Length::Fill)
        .into()
    };

    let loader_ready = match loader {
        LoaderKind::Vanilla => true,
        LoaderKind::Fabric | LoaderKind::Quilt | LoaderKind::Forge | LoaderKind::NeoForge => {
            !loader_versions_busy && !loader_versions.is_empty()
        }
    };

    let install_message = if busy || versions.is_empty() || !loader_ready {
        Message::Noop
    } else {
        Message::CreateInstanceSubmit
    };
    let close_message = if busy {
        Message::Noop
    } else {
        Message::CreateInstanceCancel
    };
    let pause_message = if paused {
        Message::CreateInstallResume
    } else {
        Message::CreateInstallPause
    };

    let dialog = container(
        column![
            row![
                column![
                    text("New instance").size(22),
                    text("Choose a Minecraft version, loader, then install.").size(13),
                ]
                .spacing(4),
                Space::with_width(Length::Fill),
                icon_button(
                    icons::CLOSE,
                    18.0,
                    close_message.clone(),
                    theme::secondary_button
                ),
            ]
            .align_y(Alignment::Center),
            text_input("Instance name", name)
                .on_input(Message::CreateInstanceNameChanged)
                .style(theme::input)
                .padding(12),
            version_pick,
            styled_pick_list(
                [
                    LoaderKind::Vanilla,
                    LoaderKind::Fabric,
                    LoaderKind::Forge,
                    LoaderKind::NeoForge,
                    LoaderKind::Quilt,
                ],
                Some(loader),
                Message::CreateInstanceLoaderChanged,
            )
            .placeholder("Loader")
            .width(Length::Fill),
            loader_version_pick,
            text(status).size(13),
            progress_bar(0.0..=1.0, progress)
                .style(theme::progress)
                .width(Length::Fill),
            row![
                button(if busy { "Abort" } else { "Cancel" })
                    .on_press(if busy {
                        Message::CreateInstallCancel
                    } else {
                        close_message
                    })
                    .style(theme::secondary_button),
                button(if paused { "Resume" } else { "Pause" })
                    .on_press(if busy { pause_message } else { Message::Noop })
                    .style(theme::secondary_button),
                Space::with_width(Length::Fill),
                button(if busy { "Installing..." } else { "Install" })
                    .on_press(install_message)
                    .style(theme::primary_button),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(14),
    )
    .width(460)
    .padding(20)
    .style(theme::shell);

    container(dialog)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::scrim)
        .into()
}

fn import_instance_overlay(path: &str, busy: bool) -> Element<'_, Message> {
    let close_message = if busy {
        Message::Noop
    } else {
        Message::ImportInstanceCancel
    };
    let import_message = if busy {
        Message::Noop
    } else {
        Message::ImportInstanceSubmit
    };
    let dialog = container(
        column![
            row![
                column![
                    text("Import instance").size(22),
                    text("Choose a Swift zip, Modrinth .mrpack, or Prism/MultiMC zip.").size(13),
                ]
                .spacing(4),
                Space::with_width(Length::Fill),
                icon_button(
                    icons::CLOSE,
                    18.0,
                    close_message.clone(),
                    theme::secondary_button
                ),
            ]
            .align_y(Alignment::Center),
            text_input("/path/to/instance.zip or pack.mrpack", path)
                .on_input(Message::ImportPathChanged)
                .style(theme::input)
                .padding(12),
            row![
                button("Cancel")
                    .on_press(close_message)
                    .style(theme::secondary_button),
                button("Choose")
                    .on_press(if busy {
                        Message::Noop
                    } else {
                        Message::PickImportZip
                    })
                    .style(theme::secondary_button),
                Space::with_width(Length::Fill),
                button(if busy { "Importing..." } else { "Import" })
                    .on_press(import_message)
                    .style(theme::primary_button),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(14),
    )
    .width(460)
    .padding(20)
    .style(theme::shell);

    container(dialog)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::scrim)
        .into()
}

fn delete_confirm_overlay<'a>(id: &'a str, name: &'a str) -> Element<'a, Message> {
    let dialog = container(
        column![
            text("Delete instance").size(22),
            text(format!("Remove {name} from Swift Launcher? Choose whether to keep or delete the instance folder.")).size(13),
            row![
                button("Cancel")
                    .on_press(Message::CancelDeleteInstance)
                    .style(theme::secondary_button),
                Space::with_width(Length::Fill),
                button("Metadata only")
                    .on_press(Message::DeleteInstance(id.to_string()))
                    .style(theme::secondary_button),
                button("Delete files")
                    .on_press(Message::DeleteInstanceWithFiles(id.to_string()))
                    .style(theme::danger_button),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(14),
    )
    .width(520)
    .padding(20)
    .style(theme::shell);

    container(dialog)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::scrim)
        .into()
}
