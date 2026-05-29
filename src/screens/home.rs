use std::collections::HashMap;
use std::path::PathBuf;

use iced::widget::{
    button, column, container, image, pick_list, progress_bar, row, scrollable, stack, text,
    text_input, Space,
};
use iced::{Alignment, Element, Length};

use crate::auth::Session;
use crate::icons::{self, icon_button, icon_button_maybe, svg_icon};
use crate::instances::screenshots::latest_screenshot;
use crate::instances::{Instance, InstanceRunState, InstanceTab, LoaderKind, SortMode};
use crate::messages::Message;
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
    search: &'a str,
    sort: SortMode,
    list_view: bool,
    settings_open: bool,
    settings: &'a LauncherSettings,
    java_status: &'a str,
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
    // Chrome removed.

    let nav = row![
        column![
            text("Swift Launcher").size(18),
            text("Minecraft instances").size(11)
        ]
        .spacing(1),
        Space::with_width(Length::Fill),
        text_input("Search instances", search)
            .on_input(Message::SearchChanged)
            .style(theme::input)
            .padding(9)
            .width(Length::FillPortion(2)),
        account_chip(session, avatar_cache, username),
        icon_button(
            icons::SETTINGS,
            18.0,
            Message::SettingsOpened,
            theme::secondary_button
        ),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let action = row![
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
        Space::with_width(Length::Fill),
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
    .align_y(Alignment::Center);

    let filtered = filtered_instances(instances, search, sort);
    let columns = grid_columns(window_width, list_view);
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

    let mut content = column![nav].spacing(14).padding(20);
    if let Some(error) = error_banner {
        content = content.push(error_row(error));
    }
    content = content.push(action).push(grid);
    if account_menu_open {
        content = content.push(account_switcher(accounts, session));
    }

    let shell = container(content).width(Length::Fill).height(Length::Fill);

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
    } else {
        base
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
    let mut list = column![text("Accounts").size(18),].spacing(10);

    if accounts.is_empty() {
        list = list.push(text("No saved accounts yet").size(13));
    }

    for account in accounts {
        let is_active = active.is_some_and(|session| session.uuid == account.uuid);
        list = list.push(
            row![
                column![
                    text(format!(
                        "{}{}",
                        account.username,
                        if is_active { " (active)" } else { "" }
                    ))
                    .size(14),
                    text(format!("{} • {}", account.provider, account.uuid)).size(11),
                ]
                .spacing(3),
                Space::with_width(Length::Fill),
                button("Use")
                    .on_press(Message::AccountSelected(account.uuid.clone()))
                    .style(theme::secondary_button),
                button("Sign out")
                    .on_press(Message::SignOut(account.uuid.clone()))
                    .style(theme::danger_button),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }

    container(list)
        .padding(14)
        .style(theme::card)
        .width(Length::Fill)
        .into()
}

fn filtered_instances<'a>(
    instances: &'a [Instance],
    search: &str,
    sort: SortMode,
) -> Vec<&'a Instance> {
    let mut filtered: Vec<_> = instances
        .iter()
        .filter(|instance| {
            instance
                .name
                .to_lowercase()
                .contains(&search.to_lowercase())
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
                    badge(loader_label(instance.loader)),
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
