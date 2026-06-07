use iced::border;
use iced::widget::{
    button, checkbox as checkbox_widget, container, overlay, progress_bar,
    scrollable as scrollable_widget, slider as slider_widget, text_input,
};
use iced::{Background, Border, Color, Padding, Shadow, Theme};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    Dark,
    Light,
    System,
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dark => f.write_str("Dark"),
            Self::Light => f.write_str("Light"),
            Self::System => f.write_str("System"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Accent {
    Indigo,
    Green,
    Orange,
    Pink,
    Cyan,
    Red,
}

impl std::fmt::Display for Accent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Indigo => f.write_str("Indigo"),
            Self::Green => f.write_str("Green"),
            Self::Orange => f.write_str("Orange"),
            Self::Pink => f.write_str("Pink"),
            Self::Cyan => f.write_str("Cyan"),
            Self::Red => f.write_str("Red"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SwiftTheme {
    pub mode: ThemeMode,
    pub accent: Accent,
}

impl Default for SwiftTheme {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            accent: Accent::Green,
        }
    }
}

impl SwiftTheme {
    pub fn iced_theme(self) -> Theme {
        match self.mode {
            ThemeMode::Light => Theme::Light,
            ThemeMode::Dark | ThemeMode::System => Theme::Dark,
        }
    }

    pub fn palette(self) -> Palette {
        let accent = match self.accent {
            Accent::Indigo => color(0xad, 0xc6, 0xff),
            Accent::Green => color(0x9b, 0xed, 0x04),
            Accent::Orange => color(0xff, 0xc1, 0x7a),
            Accent::Pink => color(0xff, 0xb2, 0xb7),
            Accent::Cyan => color(0x72, 0xdc, 0xff),
            Accent::Red => color(0xff, 0x78, 0x86),
        };

        match self.mode {
            ThemeMode::Light => Palette {
                background: color(0x13, 0x13, 0x13),
                mantle: color(0x0e, 0x0e, 0x0e),
                crust: color(0x0a, 0x0a, 0x0a),
                surface: color(0x1c, 0x1b, 0x1b),
                surface_high: color(0x2a, 0x2a, 0x2a),
                border: color(0x3c, 0x4a, 0x42),
                accent,
                success: color(0x9b, 0xed, 0x04),
                danger: color(0xff, 0x78, 0x86),
                warning: color(0xff, 0xb8, 0x6c),
                text: color(0xe5, 0xe2, 0xe1),
                muted: color(0xbb, 0xca, 0xbf),
            },
            ThemeMode::Dark | ThemeMode::System => Palette {
                background: color(0x13, 0x13, 0x13),
                mantle: color(0x0e, 0x0e, 0x0e),
                crust: color(0x0a, 0x0a, 0x0a),
                surface: color(0x1c, 0x1b, 0x1b),
                surface_high: color(0x2a, 0x2a, 0x2a),
                border: color(0x3c, 0x4a, 0x42),
                accent,
                success: color(0x9b, 0xed, 0x04),
                danger: color(0xff, 0x78, 0x86),
                warning: color(0xff, 0xb8, 0x6c),
                text: color(0xe5, 0xe2, 0xe1),
                muted: color(0xbb, 0xca, 0xbf),
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub background: Color,
    pub mantle: Color,
    pub crust: Color,
    pub surface: Color,
    pub surface_high: Color,
    pub border: Color,
    pub accent: Color,
    pub success: Color,
    pub danger: Color,
    pub warning: Color,
    pub text: Color,
    pub muted: Color,
}

pub const DARK: SwiftTheme = SwiftTheme {
    mode: ThemeMode::Dark,
    accent: Accent::Green,
};

pub fn color(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

pub fn app_container(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.background)),
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

pub fn surface(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.surface)),
        border: border::rounded(8)
            .color(Color {
                a: 0.70,
                ..p.border
            })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn shell(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.surface)),
        border: border::rounded(12)
            .color(Color {
                a: 0.80,
                ..p.border
            })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn toolbar(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.mantle)),
        border: border::rounded(8)
            .color(Color {
                a: 0.55,
                ..p.border
            })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn sidebar(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.mantle)),
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

pub fn card(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.surface)),
        border: border::rounded(8)
            .color(Color {
                a: 0.72,
                ..p.border
            })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn badge(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.muted),
        background: Some(Background::Color(p.surface_high)),
        border: border::rounded(99)
            .color(Color {
                a: 0.55,
                ..p.border
            })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn danger_badge(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(color(0xff, 0xff, 0xff)),
        background: Some(Background::Color(p.danger)),
        border: border::rounded(99)
            .color(Color {
                a: 0.72,
                ..p.danger
            })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn active_badge(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.success),
        background: Some(Background::Color(Color {
            a: 0.16,
            ..p.success
        })),
        border: border::rounded(99)
            .color(Color {
                a: 0.52,
                ..p.success
            })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn auth_active_badge(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(Color { a: 0.70, ..p.crust })),
        border: border::rounded(99)
            .color(Color { a: 0.86, ..p.text })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn inactive_badge(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.muted),
        background: Some(Background::Color(Color {
            a: 0.18,
            ..p.surface_high
        })),
        border: border::rounded(99)
            .color(Color {
                a: 0.40,
                ..p.border
            })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn survival_badge(_: &Theme) -> container::Style {
    colored_badge(color(0xa6, 0xe3, 0x7a))
}

pub fn creative_badge(_: &Theme) -> container::Style {
    colored_badge(color(0x89, 0xd6, 0xff))
}

pub fn hardcore_badge(_: &Theme) -> container::Style {
    colored_badge(color(0xff, 0x78, 0x86))
}

pub fn adventure_badge(_: &Theme) -> container::Style {
    colored_badge(color(0xff, 0xc1, 0x7a))
}

pub fn spectator_badge(_: &Theme) -> container::Style {
    colored_badge(color(0xd7, 0xbd, 0xff))
}

pub fn cheats_badge(_: &Theme) -> container::Style {
    colored_badge(DARK.palette().warning)
}

fn colored_badge(fg: Color) -> container::Style {
    container::Style {
        text_color: Some(fg),
        background: Some(Background::Color(Color { a: 0.13, ..fg })),
        border: border::rounded(99).color(Color { a: 0.70, ..fg }).width(1),
        shadow: Shadow::default(),
    }
}

pub fn banner(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(Color {
            a: 0.20,
            ..p.danger
        })),
        border: border::rounded(8).color(p.danger).width(1),
        shadow: Shadow::default(),
    }
}

pub fn scrim(_: &Theme) -> container::Style {
    container::Style {
        text_color: Some(DARK.palette().text),
        background: Some(Background::Color(Color::from_rgba(0.02, 0.02, 0.02, 0.78))),
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

pub fn primary_button(_: &Theme, status: button::Status) -> button::Style {
    button_style(DARK.palette().accent, status)
}

pub fn success_button(_: &Theme, status: button::Status) -> button::Style {
    button_style(DARK.palette().success, status)
}

pub fn danger_button(_: &Theme, status: button::Status) -> button::Style {
    button_style(DARK.palette().danger, status)
}

pub fn secondary_button(_: &Theme, status: button::Status) -> button::Style {
    let p = DARK.palette();
    let bg = match status {
        button::Status::Hovered => p.surface_high,
        button::Status::Pressed => p.surface,
        button::Status::Disabled => Color {
            a: 0.45,
            ..p.surface
        },
        button::Status::Active => p.surface,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: p.text,
        border: border::rounded(8).color(p.border).width(1),
        shadow: Shadow::default(),
    }
}

pub fn ghost_button(_: &Theme, status: button::Status) -> button::Style {
    let p = DARK.palette();
    let bg = match status {
        button::Status::Hovered => Color {
            a: 0.42,
            ..p.surface_high
        },
        button::Status::Pressed => Color {
            a: 0.34,
            ..p.surface
        },
        button::Status::Disabled | button::Status::Active => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: p.text,
        border: border::rounded(6).color(Color::TRANSPARENT).width(1),
        shadow: Shadow::default(),
    }
}

pub fn nav_button(_: &Theme, status: button::Status) -> button::Style {
    let p = DARK.palette();
    let bg = match status {
        button::Status::Hovered => p.surface_high,
        button::Status::Pressed => p.surface,
        button::Status::Disabled | button::Status::Active => p.surface,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: p.accent,
        border: border::rounded(8)
            .color(Color {
                a: 0.55,
                ..p.border
            })
            .width(1),
        shadow: Shadow::default(),
    }
}

fn button_style(base: Color, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color { a: 0.92, ..base },
        button::Status::Pressed => Color { a: 0.78, ..base },
        button::Status::Disabled => Color { a: 0.38, ..base },
        button::Status::Active => base,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: color(0x00, 0x38, 0x24),
        border: border::rounded(8).color(Color { a: 0.55, ..base }).width(1),
        shadow: Shadow::default(),
    }
}

pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let p = DARK.palette();
    let border_color = match status {
        text_input::Status::Focused => p.accent,
        text_input::Status::Hovered => p.border,
        text_input::Status::Active | text_input::Status::Disabled => p.border,
    };
    text_input::Style {
        background: Background::Color(p.surface),
        border: border::rounded(8).color(border_color).width(1),
        icon: p.muted,
        placeholder: p.muted,
        value: p.text,
        selection: p.accent,
    }
}

pub fn progress(_: &Theme) -> progress_bar::Style {
    let p = DARK.palette();
    progress_bar::Style {
        background: Background::Color(p.surface_high),
        bar: Background::Color(p.accent),
        border: border::rounded(6).color(p.border).width(1),
    }
}

pub fn slider(_: &Theme, status: slider_widget::Status) -> slider_widget::Style {
    let p = DARK.palette();
    let accent = match status {
        slider_widget::Status::Active => p.accent,
        slider_widget::Status::Hovered => p.success,
        slider_widget::Status::Dragged => p.success,
    };
    slider_widget::Style {
        rail: slider_widget::Rail {
            backgrounds: (
                Background::Color(accent),
                Background::Color(Color {
                    a: 0.70,
                    ..p.surface_high
                }),
            ),
            width: 6.0,
            border: border::rounded(99)
                .color(Color {
                    a: 0.55,
                    ..p.border
                })
                .width(1),
        },
        handle: slider_widget::Handle {
            shape: slider_widget::HandleShape::Circle { radius: 9.0 },
            background: Background::Color(accent),
            border_width: 2.0,
            border_color: p.background,
        },
    }
}

pub fn checkbox(_: &Theme, status: checkbox_widget::Status) -> checkbox_widget::Style {
    let p = DARK.palette();
    let (checked, hovered, disabled) = match status {
        checkbox_widget::Status::Active { is_checked } => (is_checked, false, false),
        checkbox_widget::Status::Hovered { is_checked } => (is_checked, true, false),
        checkbox_widget::Status::Disabled { is_checked } => (is_checked, false, true),
    };
    let bg = if checked {
        if hovered {
            p.success
        } else {
            p.accent
        }
    } else if hovered {
        p.surface_high
    } else {
        p.surface
    };
    checkbox_widget::Style {
        background: Background::Color(if disabled {
            Color { a: 0.45, ..bg }
        } else {
            bg
        }),
        icon_color: if checked { p.background } else { p.muted },
        border: border::rounded(5)
            .color(if checked { p.accent } else { p.border })
            .width(1),
        text_color: Some(if disabled { p.muted } else { p.text }),
    }
}

pub fn scrollbar_gutter() -> Padding {
    Padding {
        right: 18.0,
        ..Padding::ZERO
    }
}

pub fn scrollable(_: &Theme, status: scrollable_widget::Status) -> scrollable_widget::Style {
    let p = DARK.palette();
    let active_scroller = match status {
        scrollable_widget::Status::Active => p.accent,
        scrollable_widget::Status::Hovered { .. } | scrollable_widget::Status::Dragged { .. } => {
            p.success
        }
    };
    let rail = scrollable_widget::Rail {
        background: Some(Background::Color(Color {
            a: 0.22,
            ..p.surface_high
        })),
        border: border::rounded(99)
            .color(Color {
                a: 0.45,
                ..p.border
            })
            .width(1),
        scroller: scrollable_widget::Scroller {
            color: active_scroller,
            border: border::rounded(99)
                .color(Color {
                    a: 0.60,
                    ..active_scroller
                })
                .width(1),
        },
    };
    scrollable_widget::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: Some(Background::Color(p.surface_high)),
    }
}

pub fn pick_list(
    _: &Theme,
    status: iced::widget::pick_list::Status,
) -> iced::widget::pick_list::Style {
    let p = DARK.palette();
    let (background, border_color) = match status {
        iced::widget::pick_list::Status::Active => (p.mantle, p.border),
        iced::widget::pick_list::Status::Hovered => (p.surface, p.accent),
        iced::widget::pick_list::Status::Opened => (p.surface_high, p.accent),
    };
    iced::widget::pick_list::Style {
        text_color: p.text,
        placeholder_color: p.muted,
        handle_color: p.accent,
        background: Background::Color(background),
        border: border::rounded(8).color(border_color).width(1),
    }
}

pub fn pick_list_menu(_: &Theme) -> overlay::menu::Style {
    let p = DARK.palette();
    overlay::menu::Style {
        background: Background::Color(p.mantle),
        border: border::rounded(8).color(p.border).width(1),
        text_color: p.text,
        selected_text_color: color(0x1e, 0x1e, 0x2e),
        selected_background: Background::Color(p.accent),
    }
}
