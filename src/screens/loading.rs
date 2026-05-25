use iced::widget::{column, container, progress_bar, svg, text, Space};
use iced::{Alignment, Element, Length};

use crate::messages::Message;
use crate::theme;

pub fn view(progress: f32, status: &str) -> Element<'_, Message> {
    let logo = svg(svg::Handle::from_path("assets/logo.svg")).width(72).height(72);
    let loading_hint = if progress >= 0.99 {
        "Ready"
    } else if progress < 0.2 {
        "Opening storage"
    } else {
        "Loading launcher data"
    };
    let panel = container(column![
        logo,
        text("Swift Launcher").size(26),
        text(loading_hint).size(12),
        progress_bar(0.0..=1.0, progress).style(theme::progress).width(360),
        text(status).size(14),
    ]
    .spacing(16)
    .align_x(Alignment::Center))
    .padding(24)
    .width(400)
    .style(theme::shell);

    let content = column![Space::with_height(Length::Fill), panel, Space::with_height(Length::Fill)]
        .align_x(Alignment::Center);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::app_container)
        .into()
}