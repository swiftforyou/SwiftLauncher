use iced::widget::{button, container, svg};
use iced::{Element, Length};

use crate::messages::Message;

pub const ADD: &str = "assets/icons/add.svg";
pub const ALERT: &str = "assets/icons/alert.svg";
pub const ACCOUNT: &str = "assets/icons/account.svg";
pub const BACK: &str = "assets/icons/back.svg";
pub const CLOSE: &str = "assets/icons/close.svg";
pub const CREEPER: &str = "assets/icons/creeper.svg";
pub const DELETE: &str = "assets/icons/delete.svg";
pub const DOWNLOAD: &str = "assets/icons/download.svg";
pub const FOLDER: &str = "assets/icons/folder.svg";
pub const GRID_VIEW: &str = "assets/icons/grid_view.svg";
pub const IMPORT: &str = "assets/icons/import.svg";
pub const LIST_VIEW: &str = "assets/icons/list_view.svg";
pub const LOGS: &str = "assets/icons/logs.svg";
pub const MODS: &str = "assets/icons/mods.svg";
pub const PLAY: &str = "assets/icons/play.svg";
pub const SETTINGS: &str = "assets/icons/settings.svg";
pub const STOP: &str = "assets/icons/stop.svg";

pub fn svg_icon(path: &'static str, size: f32) -> Element<'static, Message> {
    svg(svg::Handle::from_path(path))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}

pub fn icon_button<'a>(
    path: &'static str,
    size: f32,
    message: Message,
    style: fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style,
) -> iced::widget::Button<'a, Message> {
    button(svg_icon(path, size))
        .on_press(message)
        .style(style)
        .padding(8)
}

pub fn icon_button_maybe<'a>(
    path: &'static str,
    size: f32,
    message: Option<Message>,
    style: fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style,
) -> iced::widget::Button<'a, Message> {
    let widget = button(svg_icon(path, size)).style(style).padding(8);
    if let Some(message) = message {
        widget.on_press(message)
    } else {
        widget
    }
}

pub fn icon_label_button<'a>(
    path: &'static str,
    size: f32,
    label: &'a str,
    message: Message,
    style: fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style,
) -> iced::widget::Button<'a, Message> {
    use iced::widget::{row, text};
    use iced::Alignment;

    button(
        row![svg_icon(path, size), text(label).size(13)]
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .on_press(message)
    .style(style)
    .padding([8, 12])
}

pub fn avatar_placeholder<'a>(username: &str, size: f32) -> Element<'a, Message> {
    let initial = username
        .chars()
        .find(|ch| !ch.is_whitespace())
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into());

    container(
        iced::widget::column![
            svg_icon(ACCOUNT, size * 0.55),
            iced::widget::text(initial).size((size * 0.35).max(12.0) as u16),
        ]
        .spacing(2)
        .align_x(iced::Alignment::Center),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .center_x(Length::Fixed(size))
    .center_y(Length::Fixed(size))
    .style(crate::theme::badge)
    .into()
}
