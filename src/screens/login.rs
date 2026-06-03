use iced::widget::{
    button, column, container, row, scrollable, stack, svg, text, text_input, Space,
};
use iced::{Alignment, Color, Element, Length};
use std::sync::LazyLock;

use crate::auth::AuthProvider;
use crate::icons::{self, png_icon, svg_icon};
use crate::messages::Message;
use crate::theme;

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    provider: AuthProvider,
    username: &'a str,
    password: &'a str,
    totp: &'a str,
    password_visible: bool,
    busy: bool,
    banner: Option<&'a str>,
    device: Option<(&'a str, &'a str)>,
    show_back: bool,
    window_width: f32,
) -> Element<'a, Message> {
    let compact = window_width < 620.0;
    let panel_padding = if compact { 12 } else { 20 };
    let logo_size = if compact { 58.0 } else { 82.0 };
    let title_size = if compact { 24 } else { 30 };
    let logo = svg(svg::Handle::from_memory(icons::LOGO))
        .width(Length::Fixed(logo_size))
        .height(Length::Fixed(logo_size));
    let hero = container(
        column![
            logo,
            text("Swift Launcher").size(title_size),
            text("Choose account provider").size(13),
            container(
                row![
                    provider_pill("Microsoft", provider == AuthProvider::Microsoft),
                    provider_pill("Ely.by", provider == AuthProvider::ElyBy),
                    provider_pill("LittleSkin", provider == AuthProvider::LittleSkin),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .center_x(Length::Fill),
        ]
        .spacing(12)
        .align_x(Alignment::Center),
    )
    .padding(22)
    .width(Length::Fill)
    .style(theme::toolbar);

    let microsoft_label = if busy && provider == AuthProvider::Microsoft {
        "Waiting for Microsoft"
    } else {
        "Continue with Microsoft"
    };
    let mut form = column![
        hero,
        provider_card(
            ProviderVisual {
                icon: &icons::AUTH_MICROSOFT,
                title: microsoft_label,
                subtitle: "Official account",
                text_color: theme::DARK.palette().crust,
            },
            provider == AuthProvider::Microsoft,
            if provider == AuthProvider::Microsoft {
                Message::SubmitLogin
            } else {
                Message::AuthProviderSelected(AuthProvider::Microsoft)
            },
            theme::primary_button,
            compact,
        ),
        provider_card(
            ProviderVisual {
                icon: &icons::AUTH_ELYBY,
                title: "Continue with Ely.by",
                subtitle: "Yggdrasil account",
                text_color: theme::DARK.palette().crust,
            },
            provider == AuthProvider::ElyBy,
            Message::AuthProviderSelected(AuthProvider::ElyBy),
            theme::success_button,
            compact,
        ),
        provider_card(
            ProviderVisual {
                icon: &icons::AUTH_LITTLESKIN,
                title: "Continue with LittleSkin",
                subtitle: "Skin system account",
                text_color: theme::DARK.palette().text,
            },
            provider == AuthProvider::LittleSkin,
            Message::AuthProviderSelected(AuthProvider::LittleSkin),
            theme::secondary_button,
            compact,
        ),
    ]
    .spacing(12)
    .align_x(Alignment::Center);

    if provider != AuthProvider::Microsoft {
        form = form
            .push(
                text_input("Username", username)
                    .on_input(Message::UsernameChanged)
                    .style(theme::input)
                    .padding(12),
            )
            .push(
                row![
                    text_input("Password", password)
                        .on_input(Message::PasswordChanged)
                        .secure(!password_visible)
                        .style(theme::input)
                        .padding(12),
                    button(if password_visible { "Hide" } else { "Show" })
                        .on_press(Message::TogglePasswordVisible)
                        .style(theme::secondary_button)
                ]
                .spacing(8),
            );

        if provider == AuthProvider::ElyBy {
            form = form.push(
                text_input("2FA Code (Optional)", totp)
                    .on_input(Message::TotpChanged)
                    .style(theme::input)
                    .padding(12),
            );
        }

        form = form.push(
            button(if busy { "Signing in..." } else { "Sign in" })
                .on_press(Message::SubmitLogin)
                .style(theme::primary_button)
                .width(Length::Fill),
        );
    }

    if let Some((code, url)) = device {
        let device_panel = container(
            column![
                text(format!("Code: {code}")).size(13),
                text(url).size(12),
                button("Copy URL")
                    .on_press(Message::CopyVerificationUrl)
                    .style(theme::secondary_button),
            ]
            .spacing(8),
        )
        .padding(12)
        .style(theme::card);
        form = form.push(device_panel);
    }

    let panel = container(form)
        .width(Length::Fill)
        .max_width(500)
        .padding(panel_padding)
        .style(theme::shell);
    let content = match banner {
        Some(message) => column![
            container(text(message).size(13),)
                .padding(12)
                .style(theme::banner),
            panel
        ]
        .spacing(16),
        None => column![panel].spacing(16),
    }
    .align_x(Alignment::Center);

    let page_content =
        container(content)
            .width(Length::Fill)
            .padding(if compact { [14, 12] } else { [24, 18] });
    let page = container(scrollable(page_content).style(theme::scrollable))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(theme::app_container);

    if show_back {
        stack![
            page,
            container(
                row![
                    button(svg_icon(icons::BACK, 18.0))
                        .on_press(Message::CancelAddAccount)
                        .style(theme::secondary_button)
                        .padding(8),
                    Space::with_width(Length::Fill),
                ]
                .padding(18)
            )
            .width(Length::Fill)
            .height(Length::Shrink),
        ]
        .into()
    } else {
        page.into()
    }
}

struct ProviderVisual<'a> {
    icon: &'static LazyLock<iced::widget::image::Handle>,
    title: &'a str,
    subtitle: &'a str,
    text_color: Color,
}

fn provider_card<'a>(
    provider: ProviderVisual<'a>,
    selected: bool,
    message: Message,
    style: fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style,
    compact: bool,
) -> iced::widget::Button<'a, Message> {
    let badge: Element<'a, Message> = if selected {
        container(text("ACTIVE").size(11))
            .padding([4, 8])
            .style(theme::active_badge)
            .into()
    } else if compact {
        Space::with_width(Length::Shrink).into()
    } else {
        container(text("READY").size(11))
            .padding([4, 8])
            .style(theme::badge)
            .into()
    };
    let icon_size = if compact { 34.0 } else { 42.0 };
    let image_size = if compact { 22.0 } else { 26.0 };
    button(
        row![
            container(png_icon(provider.icon, image_size))
                .width(Length::Fixed(icon_size))
                .height(Length::Fixed(icon_size))
                .center_x(Length::Fixed(icon_size))
                .center_y(Length::Fixed(icon_size))
                .style(theme::card),
            column![
                text(provider.title)
                    .size(if compact { 14 } else { 15 })
                    .color(provider.text_color),
                text(provider.subtitle)
                    .size(if compact { 11 } else { 12 })
                    .color(Color {
                        a: 0.78,
                        ..provider.text_color
                    }),
            ]
            .spacing(3),
            Space::with_width(Length::Fill),
            badge,
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .on_press(message)
    .style(style)
    .width(Length::Fill)
    .padding(10)
}

fn provider_pill(label: &'static str, selected: bool) -> Element<'static, Message> {
    container(text(label).size(11))
        .padding([5, 9])
        .style(if selected {
            theme::active_badge
        } else {
            theme::badge
        })
        .into()
}
