#![allow(dead_code)]

mod app;
mod auth;
mod download;
mod error;
mod icons;
mod instances;
mod messages;
mod screens;
mod state;
mod storage;
mod system;
mod theme;

use app::SwiftLauncher;
use iced::{window, Size};
use tracing_subscriber::EnvFilter;

fn main() -> iced::Result {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("swift_launcher=info,warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    iced::application("Swift Launcher", SwiftLauncher::update, SwiftLauncher::view)
        .subscription(SwiftLauncher::subscription)
        .theme(|app| app.theme.iced_theme())
        .window(window::Settings {
            size: Size::new(1160.0, 760.0),
            min_size: Some(Size::new(860.0, 560.0)),
            ..window::Settings::default()
        })
        .run_with(SwiftLauncher::new)
}
