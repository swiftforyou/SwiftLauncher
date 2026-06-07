use std::collections::HashMap;
use std::path::PathBuf;

use iced::widget::{
    button, column, container, image, markdown, pick_list, progress_bar, row, scrollable, stack,
    text, text_input, Space,
};
use iced::{Alignment, Element, Length, Theme as IcedTheme};

use crate::auth::Session;
use crate::icons::{self, icon_button, icon_button_maybe, svg_icon};
use crate::instances::mods::{InstalledMod, ModrinthKind, ModrinthProject, ResourceProvider};
use crate::instances::screenshots::latest_screenshot;
use crate::instances::{Instance, InstanceRunState, InstanceTab, LoaderKind, SortMode};
use crate::messages::{LauncherPage, Message};
use crate::storage::settings::LauncherSettings;
use crate::system::SystemTelemetry;
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

struct InstanceSelection<'a> {
    instances: &'a [Instance],
    kind: ModrinthKind,
    project_loaders: &'a [String],
    project_id: &'a str,
    installing: bool,
    install_status: &'a str,
    install_progress: f32,
    selected_instance_name: Option<&'a str>,
    installed_targets: &'a [(String, String)],
}

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    session: Option<&'a Session>,
    instances: &'a [Instance],
    avatar_cache: &'a HashMap<String, PathBuf>,
    window_width: f32,
    system_telemetry: &'a SystemTelemetry,
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
    discover_loader: LoaderKind,
    discover_version: &'a str,
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
    instance_selection_modal_open: bool,
    pending_install_project_id: Option<&'a str>,
    pending_install_kind: ModrinthKind,
    pending_install_project_loaders: &'a [String],
    instance_selection_installing: bool,
    instance_selection_install_status: &'a str,
    instance_selection_install_progress: f32,
    instance_selection_selected_instance: Option<&'a str>,
    pending_install_targets: &'a [(String, String)],
    installed_mods: &'a [InstalledMod],
) -> Element<'a, Message> {
    let username = session.map(|s| s.username.as_str()).unwrap_or("Guest");
    let compact = window_width < 1320.0;

    let filtered = filtered_instances(instances, search, sort, loader_filter);
    let content_width = if compact {
        (window_width - 92.0).max(320.0)
    } else {
        (window_width - 260.0).max(360.0)
    };
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
        if filtered.len() > columns * 2 {
            scrollable(
                container(rows)
                    .padding(theme::scrollbar_gutter())
                    .width(Length::Fill),
            )
            .height(Length::Fill)
            .style(theme::scrollable)
            .into()
        } else {
            container(rows).height(Length::Fill).into()
        }
    };

    let active_instance = filtered.first().copied().or_else(|| instances.first());
    let topbar = topbar(
        search,
        active_instance,
        session,
        avatar_cache,
        username,
        compact,
    );
    let sidebar = sidebar(username, session, avatar_cache, page, compact);
    let mut content = column![topbar]
        .spacing(if compact { 14 } else { 18 })
        .padding(if compact { [14, 16] } else { [18, 24] });
    if let Some(error) = error_banner {
        content = content.push(error_row(error));
    }
    content = match page {
        LauncherPage::Home => content.push(home_dashboard(
            active_instance,
            instances,
            system_telemetry,
            compact,
        )),
        LauncherPage::Instances => {
            content.push(instances_page(sort, list_view, loader_filter, grid))
        }
        LauncherPage::Discover => content.push(discover_page(
            resource_provider,
            modrinth_query,
            modrinth_kind,
            discover_loader,
            discover_version,
            create_versions,
            modrinth_results,
            modrinth_detail,
            modrinth_markdown,
            modrinth_detail_busy,
            modrinth_install_status,
            modrinth_install_progress,
            modrinth_busy,
            installed_mods,
            compact,
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
    } else if instance_selection_modal_open {
        if let Some(project_id) = pending_install_project_id {
            stack![
                base,
                instance_selection_modal(InstanceSelection {
                    instances,
                    kind: pending_install_kind,
                    project_loaders: pending_install_project_loaders,
                    project_id,
                    installing: instance_selection_installing,
                    install_status: instance_selection_install_status,
                    install_progress: instance_selection_install_progress,
                    selected_instance_name: instance_selection_selected_instance,
                    installed_targets: pending_install_targets,
                })
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            base
        }
    } else {
        base
    }
}

fn sidebar<'a>(
    username: &'a str,
    session: Option<&'a Session>,
    avatar_cache: &'a HashMap<String, PathBuf>,
    page: LauncherPage,
    compact: bool,
) -> Element<'a, Message> {
    let nav = column![
        sidebar_item(
            icons::HOME,
            "Home",
            page == LauncherPage::Home,
            Message::LauncherPageSelected(LauncherPage::Home),
            compact
        ),
        sidebar_item(
            icons::GRID_VIEW,
            "Instances",
            page == LauncherPage::Instances,
            Message::LauncherPageSelected(LauncherPage::Instances),
            compact
        ),
        sidebar_item(
            icons::MODS,
            "Discover",
            page == LauncherPage::Discover,
            Message::LauncherPageSelected(LauncherPage::Discover),
            compact
        ),
        sidebar_item(
            icons::ACCOUNT,
            "Accounts",
            page == LauncherPage::Accounts,
            Message::LauncherPageSelected(LauncherPage::Accounts),
            compact
        ),
        sidebar_item(
            icons::SETTINGS,
            "Settings",
            page == LauncherPage::Settings,
            Message::LauncherPageSelected(LauncherPage::Settings),
            compact,
        ),
    ]
    .spacing(8);

    let brand: Element<'a, Message> = if compact {
        container(svg_icon(icons::LOGO, 34.0))
            .center_x(Length::Fill)
            .into()
    } else {
        row![
            svg_icon(icons::LOGO, 38.0),
            column![
                text("Swift Launcher")
                    .size(22)
                    .color(theme::DARK.palette().accent),
                text(format!("v{}", env!("CARGO_PKG_VERSION")))
                    .size(12)
                    .color(theme::DARK.palette().muted),
            ]
            .spacing(5),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    };

    container(
        column![
            brand,
            nav,
            Space::with_height(Length::Fill),
            if compact {
                avatar_only(session, avatar_cache, username, 36.0)
            } else {
                account_chip(session, avatar_cache, username)
            },
        ]
        .spacing(if compact { 18 } else { 28 }),
    )
    .width(Length::Fixed(if compact { 76.0 } else { 240.0 }))
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
    compact: bool,
) -> Element<'static, Message> {
    let style = if active {
        theme::nav_button
    } else {
        theme::ghost_button
    };
    let content: Element<'static, Message> = if compact {
        container(svg_icon(icon, 18.0))
            .width(Length::Fill)
            .center_x(Length::Fill)
            .into()
    } else {
        row![svg_icon(icon, 18.0), text(label).size(15)]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
    };
    button(content)
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
    compact: bool,
) -> Element<'a, Message> {
    let launch_message = active_instance
        .map(|instance| match instance.run_state {
            InstanceRunState::Running | InstanceRunState::Preparing => {
                Message::StopInstance(instance.id.clone())
            }
            InstanceRunState::Idle => Message::PlayInstance(instance.id.clone()),
        })
        .unwrap_or(Message::Noop);
    let launch_is_stop = active_instance.is_some_and(|instance| {
        matches!(
            instance.run_state,
            InstanceRunState::Running | InstanceRunState::Preparing
        )
    });

    container(
        row![
            text_input("Search instances, mods, accounts...", search)
                .on_input(Message::SearchChanged)
                .style(theme::input)
                .padding(10)
                .width(Length::Fill),
            Space::with_width(Length::Fill),
            if compact {
                Element::from(Space::with_width(0))
            } else {
                icon_button(icons::ALERT, 18.0, Message::Noop, theme::ghost_button).into()
            },
            if compact {
                Element::from(Space::with_width(0))
            } else {
                icon_button(icons::LOGS, 18.0, Message::Noop, theme::ghost_button).into()
            },
            if compact {
                avatar_only(session, avatar_cache, username, 36.0)
            } else {
                account_chip(session, avatar_cache, username)
            },
            button(if compact {
                svg_icon(
                    if launch_is_stop {
                        icons::STOP
                    } else {
                        icons::PLAY
                    },
                    18.0,
                )
            } else {
                row![
                    svg_icon(
                        if launch_is_stop {
                            icons::STOP
                        } else {
                            icons::PLAY
                        },
                        18.0
                    ),
                    text(if launch_is_stop {
                        "Stop Game"
                    } else {
                        "Launch Game"
                    })
                    .size(15)
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into()
            },)
            .on_press(launch_message)
            .style(if launch_is_stop {
                theme::danger_button
            } else {
                theme::primary_button
            })
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
    system_telemetry: &'a SystemTelemetry,
    compact: bool,
) -> Element<'a, Message> {
    let hero = match active {
        Some(instance) => active_instance_hero(instance),
        None => empty_hero(),
    };
    let recent = recent_instances_panel(instances, compact);
    let layout = if compact {
        column![
            row![
                container(hero)
                    .height(Length::Fixed(250.0))
                    .width(Length::FillPortion(2)),
                container(recent)
                    .height(Length::Fixed(250.0))
                    .width(Length::FillPortion(1)),
            ]
            .spacing(12),
            row![
                container(system_panel(instances.len(), system_telemetry))
                    .height(Length::Fixed(190.0))
                    .width(Length::FillPortion(1)),
                container(up_next_panel(instances))
                    .height(Length::Fixed(190.0))
                    .width(Length::FillPortion(1)),
            ]
            .spacing(12),
            container(weekly_picks_panel())
                .height(Length::Fixed(170.0))
                .width(Length::Fill),
            recent_activity_panel(instances),
        ]
        .spacing(12)
        .width(Length::Fill)
    } else {
        let upper = row![hero, recent].spacing(18).height(Length::Fixed(292.0));
        let middle = row![
            system_panel(instances.len(), system_telemetry),
            up_next_panel(instances),
            weekly_picks_panel(),
        ]
        .spacing(18)
        .height(Length::Fixed(230.0));
        column![upper, middle, recent_activity_panel(instances)]
            .spacing(18)
            .width(Length::Fill)
    };

    scrollable(
        container(layout)
            .padding(theme::scrollbar_gutter())
            .width(Length::Fill),
    )
    .height(Length::Fill)
    .style(theme::scrollable)
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

#[allow(clippy::too_many_arguments)]
fn discover_page<'a>(
    provider: ResourceProvider,
    query: &'a str,
    kind: ModrinthKind,
    discover_loader: LoaderKind,
    discover_version: &'a str,
    versions: &'a [String],
    results: &'a [ModrinthProject],
    detail: Option<&'a crate::instances::mods::ModrinthProjectDetail>,
    markdown: &'a [markdown::Item],
    detail_busy: bool,
    install_status: &'a str,
    install_progress: f32,
    busy: bool,
    installed_mods: &'a [InstalledMod],
    compact: bool,
) -> Element<'a, Message> {
    let mut result_list = column![].spacing(12);
    if detail_busy || detail.is_some() {
        result_list = result_list.push(resource_detail_page(
            detail,
            markdown,
            detail_busy,
            install_status,
            install_progress,
            installed_mods,
        ));
    } else if busy {
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
            result_list = result_list.push(discover_result_row(item, installed_mods, compact));
        }
    }

    let filters = row![
        text(provider.to_string())
            .size(if compact { 24 } else { 28 })
            .color(theme::DARK.palette().accent),
        Space::with_width(Length::Fill),
        styled_pick_list(
            ResourceProvider::ALL,
            Some(provider),
            Message::ResourceProviderSelected
        )
        .width(if compact {
            Length::FillPortion(1)
        } else {
            Length::Fixed(160.0)
        }),
        if matches!(kind, ModrinthKind::Mods | ModrinthKind::Modpacks) {
            styled_pick_list(
                LoaderKind::ALL,
                Some(discover_loader),
                Message::DiscoverLoaderSelected,
            )
            .width(if compact {
                Length::FillPortion(1)
            } else {
                Length::Fixed(140.0)
            })
            .into()
        } else {
            Element::from(Space::with_width(0))
        },
        styled_pick_list(
            versions.to_vec(),
            Some(discover_version.to_string()),
            Message::DiscoverVersionSelected,
        )
        .width(if compact {
            Length::FillPortion(1)
        } else {
            Length::Fixed(150.0)
        }),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let search_row = row![
        text_input("Search mods...", query)
            .on_input(Message::ModrinthSearchChanged)
            .on_submit(Message::SearchModrinth)
            .style(theme::input)
            .padding(10)
            .width(Length::Fill),
        button(if busy { "Searching..." } else { "Search" })
            .on_press(if busy {
                Message::Noop
            } else {
                Message::SearchModrinth
            })
            .style(theme::primary_button)
            .padding([10, 16]),
    ]
    .spacing(10);

    let result_area: Element<'a, Message> = if detail.is_none() && !busy && results.is_empty() {
        container(result_list).height(Length::Fill).into()
    } else {
        scrollable(
            container(result_list)
                .padding(discover_scroll_padding())
                .width(Length::Fill),
        )
        .height(Length::Fill)
        .style(theme::scrollable)
        .into()
    };

    let body: Element<'a, Message> = row![
        discover_kind_tabs(kind, compact),
        column![search_row, result_area,]
            .spacing(if compact { 10 } else { 14 })
            .width(Length::Fill),
    ]
    .spacing(if compact { 10 } else { 14 })
    .height(Length::Fill)
    .into();

    container(column![filters, body].spacing(16))
        .height(Length::Fill)
        .width(Length::Fill)
        .into()
}

fn discover_scroll_padding() -> iced::Padding {
    iced::Padding {
        top: 12.0,
        right: theme::scrollbar_gutter().right,
        bottom: 12.0,
        left: 2.0,
    }
}

fn resource_detail_page<'a>(
    detail: Option<&'a crate::instances::mods::ModrinthProjectDetail>,
    parsed_markdown: &'a [markdown::Item],
    busy: bool,
    install_status: &'a str,
    install_progress: f32,
    _installed_mods: &'a [InstalledMod],
) -> Element<'a, Message> {
    if busy {
        return container(
            column![
                back_row("Loading project page..."),
                container(text("Loading project page...").size(16))
                    .padding(18)
                    .style(theme::card),
            ]
            .spacing(14),
        )
        .padding(18)
        .style(theme::shell)
        .into();
    }

    if detail.is_none() {
        return container(
            column![
                back_row("Project page"),
                text("Project page unavailable").size(14)
            ]
            .spacing(14),
        )
        .padding(18)
        .style(theme::shell)
        .into();
    }

    if let Some(detail) = detail {
        let action = button(
            row![svg_icon(icons::DOWNLOAD, 15.0), text("Install").size(14)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .on_press(Message::InstallModrinthProject(detail.project_id.clone()))
        .style(theme::primary_button);
        let markdown_body = markdown::view(
            parsed_markdown,
            markdown::Settings::with_text_size(13),
            markdown::Style::from_palette(IcedTheme::Dark.palette()),
        )
        .map(|url| Message::OpenExternal(url.to_string()));
        let install_status = if install_status.trim().is_empty() {
            "Ready"
        } else {
            install_status
        };
        return container(
            column![
                back_row("Project page"),
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
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                row![
                    action.padding([8, 14]),
                    text(install_status)
                        .size(12)
                        .color(theme::DARK.palette().muted),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                progress_bar(0.0..=1.0, install_progress)
                    .height(Length::Fixed(6.0))
                    .style(theme::progress),
                scrollable(
                    container(markdown_body)
                        .padding(theme::scrollbar_gutter())
                        .width(Length::Fill),
                )
                .height(Length::Fill)
                .style(theme::scrollable),
            ]
            .spacing(14),
        )
        .padding(18)
        .style(theme::shell)
        .into();
    }

    container(text("Project page unavailable").size(14))
        .padding(18)
        .style(theme::shell)
        .into()
}

fn back_row<'a>(title: &'a str) -> Element<'a, Message> {
    row![
        icon_button(
            icons::BACK,
            18.0,
            Message::CloseModrinthProject,
            theme::secondary_button
        ),
        text(title).size(18),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn discover_kind_tabs(selected: ModrinthKind, compact: bool) -> Element<'static, Message> {
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
            button(text(label).size(if compact { 13 } else { 15 }))
                .on_press(Message::ModrinthKindSelected(kind))
                .style(style)
                .padding(if compact { [8, 10] } else { [9, 14] })
                .width(Length::Fill),
        );
    }
    container(tabs)
        .width(Length::Fixed(if compact { 126.0 } else { 150.0 }))
        .style(theme::card)
        .into()
}

fn discover_result_row<'a>(
    item: &'a ModrinthProject,
    _installed_mods: &'a [InstalledMod],
    compact: bool,
) -> Element<'a, Message> {
    let action_button = button(
        container(
            row![svg_icon(icons::DOWNLOAD, 15.0), text("Install").size(14)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .center_x(Length::Fill),
    )
    .on_press(Message::InstallModrinthProject(item.project_id.clone()))
    .style(theme::primary_button);
    let title = row![
        text(&item.title).size(if compact { 16 } else { 18 }),
        text(format!("by {}", item.author))
            .size(12)
            .color(theme::DARK.palette().muted),
    ]
    .spacing(5)
    .align_y(Alignment::Center);
    let info = column![
        title,
        text(&item.description)
            .size(13)
            .width(Length::Fill)
            .color(theme::DARK.palette().text),
        discover_meta(item),
    ]
    .spacing(8)
    .width(Length::Fill);
    let actions = column![
        text(format!("{} Downloads", format_downloads(item.downloads)))
            .size(14)
            .color(theme::DARK.palette().text),
        action_button
            .padding([8, 14])
            .width(Length::Fixed(if compact { 128.0 } else { 160.0 })),
        button(
            container(text("Open Page").size(14))
                .width(Length::Fill)
                .center_x(Length::Fill),
        )
        .on_press(Message::OpenModrinthProject(item.project_id.clone()))
        .style(theme::secondary_button)
        .padding([8, 14])
        .width(Length::Fixed(if compact { 128.0 } else { 160.0 })),
    ]
    .spacing(8)
    .align_x(Alignment::End);
    let body: Element<'a, Message> = if compact {
        row![project_icon(item.icon.as_ref(), 48.0), info, actions,]
            .spacing(10)
            .align_y(Alignment::Center)
            .into()
    } else {
        row![project_icon(item.icon.as_ref(), 64.0), info, actions,]
            .spacing(14)
            .align_y(Alignment::Center)
            .into()
    };
    container(body)
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
            .center_x(Length::Fixed(size))
            .center_y(Length::Fixed(size))
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
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect(),
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

fn relative_last_played(timestamp: Option<u64>) -> String {
    let Some(timestamp) = timestamp else {
        return "never".into();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(timestamp);
    let elapsed = now.saturating_sub(timestamp);
    if elapsed < 60 {
        "just now".into()
    } else if elapsed < 120 {
        "a minute ago".into()
    } else if elapsed < 3_600 {
        format!("{} minutes ago", elapsed / 60)
    } else if elapsed < 7_200 {
        "an hour ago".into()
    } else if elapsed < 86_400 {
        format!("{} hours ago", elapsed / 3_600)
    } else if elapsed < 172_800 {
        "a day ago".into()
    } else {
        format!("{} days ago", elapsed / 86_400)
    }
}

fn active_instance_hero(instance: &Instance) -> Element<'_, Message> {
    container(
        column![
            card_artwork(instance, 132.0),
            row![
                badge("Active Instance"),
                badge(&instance.minecraft_version),
                badge(loader_label(instance.loader)),
            ]
            .spacing(8),
            text(&instance.name).size(30),
            text(format!(
                "{} instance, {} RAM allocated. Last played {}.",
                loader_label(instance.loader),
                format_ram(instance.ram_mb),
                relative_last_played(instance.last_played_unix)
            ))
            .size(13)
            .color(theme::DARK.palette().muted),
            row![
                button(row![
                    svg_icon(
                        if matches!(
                            instance.run_state,
                            InstanceRunState::Running | InstanceRunState::Preparing
                        ) {
                            icons::STOP
                        } else {
                            icons::PLAY
                        },
                        18.0
                    ),
                    text(
                        if matches!(
                            instance.run_state,
                            InstanceRunState::Running | InstanceRunState::Preparing
                        ) {
                            "Stop"
                        } else {
                            "Play Now"
                        }
                    )
                    .size(16)
                ])
                .on_press(match instance.run_state {
                    InstanceRunState::Running | InstanceRunState::Preparing => {
                        Message::StopInstance(instance.id.clone())
                    }
                    InstanceRunState::Idle => Message::PlayInstance(instance.id.clone()),
                })
                .style(
                    if matches!(
                        instance.run_state,
                        InstanceRunState::Running | InstanceRunState::Preparing
                    ) {
                        theme::danger_button
                    } else {
                        theme::primary_button
                    }
                )
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
    .padding(18)
    .width(Length::FillPortion(3))
    .height(Length::Fill)
    .style(theme::shell)
    .into()
}

fn empty_hero<'a>() -> Element<'a, Message> {
    container(
        column![
            svg_icon(icons::LOGO, 54.0),
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

fn system_panel(instance_count: usize, telemetry: &SystemTelemetry) -> Element<'static, Message> {
    let (memory_label, memory_progress) = usage_label_and_progress(
        telemetry.memory_used_bytes,
        telemetry.memory_total_bytes,
        "Collecting...",
    );
    let (disk_label, disk_progress) = usage_label_and_progress(
        telemetry.disk_used_bytes,
        telemetry.disk_total_bytes,
        "Collecting...",
    );
    let cpu_label = telemetry
        .cpu_usage_percent
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(|| "Collecting...".into());
    let cpu_progress = telemetry
        .cpu_usage_percent
        .map(|value| value / 100.0)
        .unwrap_or_default();
    let online = telemetry.memory_total_bytes.is_some()
        || telemetry.disk_total_bytes.is_some()
        || telemetry.cpu_usage_percent.is_some();
    container(
        column![
            text("System Status")
                .size(18)
                .color(theme::DARK.palette().accent),
            text("Launcher telemetry")
                .size(12)
                .color(theme::DARK.palette().muted),
            metric_bar(
                "RAM Usage",
                memory_label,
                memory_progress,
                theme::DARK.palette().warning
            ),
            metric_bar(
                "CPU Usage",
                cpu_label,
                cpu_progress,
                theme::DARK.palette().accent
            ),
            metric_bar(
                "Disk Storage",
                disk_label,
                disk_progress,
                theme::DARK.palette().accent
            ),
            Space::with_height(Length::Fill),
            row![
                text("Instances")
                    .size(12)
                    .color(theme::DARK.palette().muted),
                Space::with_width(Length::Fill),
                text(instance_count.to_string()).size(14),
            ],
            row![
                text("Telemetry")
                    .size(12)
                    .color(theme::DARK.palette().muted),
                Space::with_width(Length::Fill),
                text(if online { "Online" } else { "Unavailable" })
                    .size(14)
                    .color(if online {
                        theme::DARK.palette().success
                    } else {
                        theme::DARK.palette().muted
                    }),
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

fn usage_label_and_progress(
    used: Option<u64>,
    total: Option<u64>,
    fallback: &'static str,
) -> (String, f32) {
    match (used, total) {
        (Some(used), Some(total)) if total > 0 => (
            format!("{} / {}", format_bytes(used), format_bytes(total)),
            (used as f32 / total as f32).clamp(0.0, 1.0),
        ),
        _ => (fallback.into(), 0.0),
    }
}

fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / GIB)
    } else if bytes >= 1024 * 1024 {
        format!("{:.0} MB", bytes as f64 / MIB)
    } else {
        format!("{} KB", bytes / 1024)
    }
}

fn recent_instances_panel<'a>(instances: &'a [Instance], compact: bool) -> Element<'a, Message> {
    let mut list = column![
        text("Recent Instances").size(18),
        text("Fast resume")
            .size(12)
            .color(theme::DARK.palette().muted),
    ]
    .spacing(10);

    for instance in recent_instances(instances).into_iter().take(3) {
        list = list.push(
            button(
                row![
                    card_artwork(instance, 42.0),
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
            .padding(7),
        );
    }

    if instances.is_empty() {
        list = list.push(text("No instances yet").size(13));
    }

    container(list)
        .padding(14)
        .width(if compact {
            Length::Fill
        } else {
            Length::Fixed(284.0)
        })
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
                        "Last played {}",
                        relative_last_played(instance.last_played_unix)
                    ))
                    .size(12)
                    .color(theme::DARK.palette().muted),
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
    value: String,
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
            icon_button(
                icons::ADD,
                18.0,
                Message::NewInstance,
                theme::primary_button
            ),
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
                .on_press(Message::InstanceLoaderFilterChanged(
                    if selected == Some(loader) {
                        None
                    } else {
                        Some(loader)
                    },
                ))
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
        svg_icon(icons::LOGO, 56.0),
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

fn avatar_only<'a>(
    session: Option<&'a Session>,
    avatar_cache: &'a HashMap<String, PathBuf>,
    username: &'a str,
    size: f32,
) -> Element<'a, Message> {
    let avatar = session.and_then(|s| {
        avatar_cache.get(&s.uuid).map(|path| {
            image(image::Handle::from_path(path))
                .width(size)
                .height(size)
                .into()
        })
    });
    let leading: Element<'a, Message> =
        avatar.unwrap_or_else(|| icons::avatar_placeholder(username, size));
    button(leading)
        .on_press(Message::AccountMenuToggled)
        .style(theme::secondary_button)
        .padding(4)
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
    let mut cards = column![].spacing(6);

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
        let privilege = if matches!(account.provider, crate::auth::AuthProvider::Microsoft) {
            "PRIVILEGE"
        } else {
            "REGULAR"
        };
        cards = cards.push(
            container(
                row![
                    container(svg_icon(icons::ACCOUNT, 17.0))
                        .width(Length::Fixed(34.0))
                        .height(Length::Fixed(34.0))
                        .center_x(Length::Fixed(34.0))
                        .center_y(Length::Fixed(34.0))
                        .style(theme::surface),
                    column![
                        row![
                            text(&account.username).size(14),
                            status_badge(if is_active { "ACTIVE" } else { "UNACTIVE" }, is_active),
                            provider_badge(privilege),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                        text(format!("{} account", account.provider))
                            .size(11)
                            .color(theme::DARK.palette().muted),
                    ]
                    .spacing(5),
                    Space::with_width(Length::Fill),
                    button(if is_active { "Current" } else { "Use" })
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
                        .padding([5, 8]),
                    button("Remove")
                        .on_press(Message::SignOut(account.uuid.clone()))
                        .style(theme::danger_button)
                        .padding([5, 8]),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([7, 9])
            .height(Length::Fixed(50.0))
            .style(theme::surface),
        );
    }

    let summary = container(
        row![
            column![
                text("Accounts")
                    .size(24)
                    .color(theme::DARK.palette().accent),
                text("Saved profiles")
                    .size(13)
                    .color(theme::DARK.palette().muted),
            ]
            .spacing(4),
            Space::with_width(Length::Fill),
            button("Add")
                .on_press(Message::AddAccount)
                .style(theme::primary_button)
                .padding([8, 12]),
        ]
        .align_y(Alignment::Center),
    )
    .padding(14)
    .style(theme::shell)
    .width(Length::Fill);

    column![
        summary,
        container(cards)
            .style(theme::card)
            .padding(8)
            .width(Length::Fill)
    ]
    .spacing(10)
    .into()
}

fn status_badge<'a>(label: &'a str, active: bool) -> Element<'a, Message> {
    container(text(label).size(10))
        .padding([2, 7])
        .style(if active {
            theme::active_badge
        } else {
            theme::inactive_badge
        })
        .into()
}

fn provider_badge<'a>(label: &'a str) -> Element<'a, Message> {
    container(text(label).size(10))
        .padding([2, 7])
        .style(theme::badge)
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
        InstanceRunState::Preparing => icon_button_maybe(
            icons::STOP,
            18.0,
            Some(Message::StopInstance(instance.id.clone())),
            theme::danger_button,
        )
        .into(),
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
                    icons::MODS,
                    18.0,
                    Message::OpenInstanceTab(instance.id.clone(), InstanceTab::Mods),
                    theme::secondary_button,
                ),
                icon_button(
                    icons::WORLD,
                    18.0,
                    Message::OpenInstanceTab(instance.id.clone(), InstanceTab::Worlds),
                    theme::secondary_button,
                ),
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
                        icons::MODS,
                        18.0,
                        Message::OpenInstanceTab(instance.id.clone(), InstanceTab::Mods),
                        theme::secondary_button,
                    ),
                    icon_button(
                        icons::WORLD,
                        18.0,
                        Message::OpenInstanceTab(instance.id.clone(), InstanceTab::Worlds),
                        theme::secondary_button,
                    ),
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
        container(
            image(image::Handle::from_bytes(icons::INSTANCE_BANNER.to_vec()))
                .width(Length::Fill)
                .height(height)
                .content_fit(iced::ContentFit::Cover),
        )
        .height(height)
        .clip(true)
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

fn installing_progress_modal<'a>(
    kind: ModrinthKind,
    instance_name: &'a str,
    status: &'a str,
    progress: f32,
) -> Element<'a, Message> {
    let is_complete = progress >= 1.0;
    let is_error = status.to_ascii_lowercase().contains("failed");
    let (status_label, status_color) = if is_complete && !is_error {
        ("Complete", theme::DARK.palette().success)
    } else if is_error {
        ("Failed", theme::DARK.palette().danger)
    } else {
        ("Installing", theme::DARK.palette().accent)
    };

    let mut content = column![row![
        column![
            text("Installing").size(22),
            text(format!("Installing {} to {}", kind, instance_name))
                .size(13)
                .color(theme::DARK.palette().muted),
        ]
        .spacing(4),
        Space::with_width(Length::Fill),
        if is_complete || is_error {
            button(svg_icon(icons::CLOSE, 18.0))
                .on_press(Message::CloseInstanceSelectionModal)
                .style(theme::secondary_button)
                .padding(8)
        } else {
            button(svg_icon(icons::CLOSE, 18.0))
                .on_press(Message::Noop)
                .style(theme::secondary_button)
                .padding(8)
        },
    ]
    .align_y(Alignment::Center)
    .spacing(12),]
    .spacing(20);

    let progress_section = container(
        column![
            row![
                column![
                    container(text(status_label).size(12).color(status_color))
                        .padding([4, 9])
                        .style(theme::badge),
                    text(status).size(15),
                ]
                .spacing(8)
                .width(Length::Fill),
                text(format!("{}%", (progress * 100.0) as u32))
                    .size(18)
                    .color(status_color),
            ]
            .align_y(Alignment::Center)
            .spacing(12),
            progress_bar(0.0..=1.0, progress)
                .height(Length::Fixed(10.0))
                .style(theme::progress),
        ]
        .spacing(16)
        .padding(20),
    )
    .style(theme::card)
    .width(Length::Fill);

    content = content.push(progress_section);

    // Action buttons
    if is_complete && !is_error {
        content = content.push(
            row![
                Space::with_width(Length::Fill),
                button("Done")
                    .on_press(Message::CloseInstanceSelectionModal)
                    .style(theme::primary_button)
                    .padding([10, 24]),
            ]
            .spacing(8),
        );
    } else if is_error {
        content = content.push(
            row![
                Space::with_width(Length::Fill),
                button("Close")
                    .on_press(Message::CloseInstanceSelectionModal)
                    .style(theme::danger_button)
                    .padding([10, 24]),
            ]
            .spacing(8),
        );
    } else {
        // Show cancel button during download
        content = content.push(
            row![
                Space::with_width(Length::Fill),
                text("Installing in progress...")
                    .size(12)
                    .color(theme::DARK.palette().muted),
            ]
            .spacing(8),
        );
    }

    let dialog = container(content)
        .width(520)
        .padding(24)
        .style(theme::shell);

    container(dialog)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::scrim)
        .into()
}

fn instance_selection_modal<'a>(selection: InstanceSelection<'a>) -> Element<'a, Message> {
    use std::collections::HashMap;

    // If installing, show progress view
    if selection.installing {
        return installing_progress_modal(
            selection.kind,
            selection.selected_instance_name.unwrap_or("Unknown"),
            selection.install_status,
            selection.install_progress,
        );
    }

    // Group instances by loader
    let mut grouped: HashMap<LoaderKind, Vec<&Instance>> = HashMap::new();

    // For resource packs and shaders, no loader filtering
    let needs_loader_filter = matches!(selection.kind, ModrinthKind::Mods | ModrinthKind::Modpacks);

    if needs_loader_filter {
        // Filter by compatible loaders
        for instance in selection.instances {
            let is_compatible = if selection.project_loaders.is_empty() {
                true
            } else {
                let instance_loader = match instance.loader {
                    LoaderKind::Fabric => "fabric",
                    LoaderKind::Quilt => "quilt",
                    LoaderKind::Forge => "forge",
                    LoaderKind::NeoForge => "neoforge",
                    LoaderKind::Vanilla => "vanilla",
                };
                selection
                    .project_loaders
                    .iter()
                    .any(|loader| loader.eq_ignore_ascii_case(instance_loader))
            };

            if is_compatible {
                grouped.entry(instance.loader).or_default().push(instance);
            }
        }
    } else {
        // For resource packs/shaders, show all instances
        grouped.insert(LoaderKind::Vanilla, selection.instances.iter().collect());
    }

    let mut content = column![row![
        column![
            text("Install target").size(22),
            text(format!("Choose an instance for this {}", selection.kind))
                .size(13)
                .color(theme::DARK.palette().muted),
        ]
        .spacing(4),
        Space::with_width(Length::Fill),
        icon_button(
            icons::CLOSE,
            18.0,
            Message::CloseInstanceSelectionModal,
            theme::secondary_button
        ),
    ]
    .align_y(Alignment::Center),]
    .spacing(14);

    if grouped.is_empty() {
        content = content.push(
            container(
                column![
                    text("No compatible instances").size(16),
                    text(format!(
                        "Create an instance with {} to install this {}",
                        if selection.project_loaders.is_empty() {
                            "any loader".to_string()
                        } else {
                            selection.project_loaders.join(", ")
                        },
                        selection.kind
                    ))
                    .size(13),
                ]
                .spacing(8)
                .align_x(Alignment::Center),
            )
            .padding(20)
            .style(theme::card),
        );
    } else {
        let mut instance_list = column![].spacing(10);

        // Sort loaders for consistent display
        let mut loader_keys: Vec<_> = grouped.keys().copied().collect();
        loader_keys.sort_by_key(|k| format!("{:?}", k));

        for loader in loader_keys {
            let instances_for_loader = grouped.get(&loader).unwrap();

            // Only show loader header if we're filtering by loader
            if needs_loader_filter {
                instance_list = instance_list.push(
                    text(format!(
                        "{} profiles ({})",
                        loader,
                        instances_for_loader.len()
                    ))
                    .size(16)
                    .color(theme::DARK.palette().accent),
                );
            }

            for instance in instances_for_loader {
                let installed = selection
                    .installed_targets
                    .iter()
                    .find(|(instance_id, _)| instance_id == &instance.id)
                    .map(|(_, mod_id)| mod_id.clone());
                let is_installed = installed.is_some();
                let (action_icon, action_message) = if let Some(mod_id) = installed {
                    (
                        icons::DELETE,
                        Message::UninstallFromInstance {
                            instance_id: instance.id.clone(),
                            mod_id,
                        },
                    )
                } else {
                    (
                        icons::DOWNLOAD,
                        Message::InstallToInstance {
                            instance_id: instance.id.clone(),
                            project_id: selection.project_id.to_string(),
                        },
                    )
                };
                let action_badge_style = if is_installed {
                    theme::danger_badge
                } else {
                    theme::badge
                };
                instance_list = instance_list.push(
                    button(
                        row![
                            card_artwork(instance, 54.0),
                            column![
                                text(&instance.name).size(16),
                                text(format!(
                                    "Minecraft {} • {}",
                                    instance.minecraft_version, instance.loader
                                ))
                                .size(12)
                                .color(theme::DARK.palette().muted),
                                row![
                                    loader_badge(instance.loader),
                                    run_state_label(instance.run_state),
                                ]
                                .spacing(6),
                            ]
                            .spacing(5)
                            .width(Length::Fill),
                            Space::with_width(Length::Fill),
                            container(svg_icon(action_icon, 16.0),)
                                .padding(10)
                                .style(action_badge_style),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center)
                        .padding(12),
                    )
                    .on_press(action_message)
                    .style(theme::secondary_button)
                    .width(Length::Fill),
                );
            }

            if needs_loader_filter {
                instance_list = instance_list.push(Space::with_height(8));
            }
        }

        content = content.push(
            container(
                scrollable(
                    container(instance_list)
                        .padding(theme::scrollbar_gutter())
                        .width(Length::Fill),
                )
                .height(Length::Fixed(420.0))
                .style(theme::scrollable),
            )
            .padding(12)
            .style(theme::card),
        );
    }

    let dialog = container(content)
        .width(500)
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
