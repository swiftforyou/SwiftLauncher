use iced::widget::{button, column, container, row, stack, svg, text, text_input, Space};
use iced::{Alignment, Element, Length};

use crate::auth::AuthProvider;
use crate::icons::{self, svg_icon};
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
) -> Element<'a, Message> {
    let logo = svg(svg::Handle::from_memory(icons::LOGO))
        .width(72)
        .height(72);
    let mut form = column![
        logo,
        text("Swift Launcher").size(26),
        text("Sign in to sync launch credentials").size(12),
        button(if busy && provider == AuthProvider::Microsoft {
            "Waiting..."
        } else {
            "Continue with Microsoft"
        })
        .on_press(if provider == AuthProvider::Microsoft {
            Message::SubmitLogin
        } else {
            Message::AuthProviderSelected(AuthProvider::Microsoft)
        })
        .style(theme::primary_button)
        .width(Length::Fill)
        .padding(12),
        provider_button(
            "Continue with Ely.by",
            AuthProvider::ElyBy,
            theme::success_button
        ),
        provider_button(
            "Continue with LittleSkin",
            AuthProvider::LittleSkin,
            theme::secondary_button
        ),
    ]
    .spacing(14)
    .align_x(Alignment::Center);

    if provider != AuthProvider::Microsoft {
        form = form
            .push(
                text_input("username", username)
                    .on_input(Message::UsernameChanged)
                    .style(theme::input)
                    .padding(12),
            )
            .push(
                row![
                    text_input("password", password)
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
                text_input("2FA code (optional)", totp)
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

    let panel = container(form).width(430).padding(22).style(theme::shell);
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

    let page = container(content)
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

fn provider_button<'a>(
    label: &'a str,
    provider: AuthProvider,
    style: fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style,
) -> iced::widget::Button<'a, Message> {
    button(text(label))
        .on_press(Message::AuthProviderSelected(provider))
        .style(style)
        .width(Length::Fill)
        .padding(12)
}
