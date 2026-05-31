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
            Accent::Green => color(0x4e, 0xde, 0xa3),
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
                success: color(0x10, 0xb9, 0x81),
                danger: color(0xff, 0x78, 0x86),
                warning: color(0xad, 0xc6, 0xff),
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
                success: color(0x10, 0xb9, 0x81),
                danger: color(0xff, 0x78, 0x86),
                warning: color(0xad, 0xc6, 0xff),
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
        border: border::rounded(8).color(Color { a: 0.70, ..p.border }).width(1),
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
            .color(Color { a: 0.72, ..p.border })
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
        border: border::rounded(8).color(Color { a: 0.55, ..p.border }).width(1),
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
