use iced::border;
use iced::widget::{button, container, overlay, progress_bar, text_input};
use iced::{Background, Border, Color, Shadow, Theme};
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
            accent: Accent::Pink,
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
            Accent::Indigo => color(0xb4, 0xbe, 0xfe),
            Accent::Green => color(0xa6, 0xe3, 0xa1),
            Accent::Orange => color(0xfa, 0xb3, 0x87),
            Accent::Pink => color(0xf5, 0xc2, 0xe7),
            Accent::Cyan => color(0x94, 0xe2, 0xd5),
            Accent::Red => color(0xf3, 0x8b, 0xa8),
        };

        match self.mode {
            ThemeMode::Light => Palette {
                background: color(0x1e, 0x1e, 0x2e),
                mantle: color(0x18, 0x18, 0x25),
                crust: color(0x11, 0x11, 0x1b),
                surface: color(0x31, 0x32, 0x44),
                surface_high: color(0x45, 0x47, 0x5a),
                border: color(0x58, 0x5b, 0x70),
                accent,
                success: color(0xa6, 0xe3, 0xa1),
                danger: color(0xf3, 0x8b, 0xa8),
                warning: color(0xf9, 0xe2, 0xaf),
                text: color(0xcd, 0xd6, 0xf4),
                muted: color(0xa6, 0xad, 0xc8),
            },
            ThemeMode::Dark | ThemeMode::System => Palette {
                background: color(0x1e, 0x1e, 0x2e),
                mantle: color(0x18, 0x18, 0x25),
                crust: color(0x11, 0x11, 0x1b),
                surface: color(0x31, 0x32, 0x44),
                surface_high: color(0x45, 0x47, 0x5a),
                border: color(0x58, 0x5b, 0x70),
                accent,
                success: color(0xa6, 0xe3, 0xa1),
                danger: color(0xf3, 0x8b, 0xa8),
                warning: color(0xf9, 0xe2, 0xaf),
                text: color(0xcd, 0xd6, 0xf4),
                muted: color(0xa6, 0xad, 0xc8),
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
    accent: Accent::Pink,
};

pub fn color(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

pub fn app_container(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.mantle)),
        border: Border::default(),
        shadow: Shadow::default(),
    }
}

pub fn surface(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.background)),
        border: border::rounded(8).color(p.border).width(1),
        shadow: Shadow::default(),
    }
}

pub fn shell(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.mantle)),
        border: border::rounded(10)
            .color(Color {
                a: 0.85,
                ..p.border
            })
            .width(1),
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.35),
            offset: iced::Vector::new(0.0, 18.0),
            blur_radius: 40.0,
        },
    }
}

pub fn toolbar(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.background)),
        border: border::rounded(8)
            .color(Color {
                a: 0.55,
                ..p.border
            })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn card(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.text),
        background: Some(Background::Color(p.surface)),
        border: border::rounded(8)
            .color(Color { a: 0.7, ..p.border })
            .width(1),
        shadow: Shadow::default(),
    }
}

pub fn badge(_: &Theme) -> container::Style {
    let p = DARK.palette();
    container::Style {
        text_color: Some(p.muted),
        background: Some(Background::Color(p.mantle)),
        border: border::rounded(99)
            .color(Color {
                a: 0.55,
                ..p.border
            })
            .width(1),
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
        background: Some(Background::Color(Color::from_rgba(0.07, 0.07, 0.11, 0.78))),
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

fn button_style(base: Color, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color { a: 0.92, ..base },
        button::Status::Pressed => Color { a: 0.78, ..base },
        button::Status::Disabled => Color { a: 0.38, ..base },
        button::Status::Active => base,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: color(0x1e, 0x1e, 0x2e),
        border: border::rounded(8).color(Color { a: 0.55, ..base }).width(1),
        shadow: Shadow::default(),
    }
}

pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let p = DARK.palette();
    let border_color = match status {
        text_input::Status::Focused => p.accent,
        text_input::Status::Hovered => p.muted,
        text_input::Status::Active | text_input::Status::Disabled => p.border,
    };
    text_input::Style {
        background: Background::Color(p.mantle),
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
        background: Background::Color(p.mantle),
        bar: Background::Color(p.accent),
        border: border::rounded(6).color(p.border).width(1),
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
