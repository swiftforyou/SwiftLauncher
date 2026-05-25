<div align="center">
  <img alt="Rust 2021" src="assets/logo.svg">
  <h1>Swift Launcher</h1>
  <p>Modern, fast Minecraft launcher built with Rust and Iced.</p>
  <p>
    <img alt="Rust 2021" src="https://img.shields.io/badge/rust-2021-333333?logo=rust&logoColor=white">
    <img alt="Iced 0.13.1" src="https://img.shields.io/badge/iced-0.13.1-5C6BC0">
    <img alt="Tokio" src="https://img.shields.io/badge/tokio-1.52-222222?logo=tokio&logoColor=white">
    <img alt="Reqwest" src="https://img.shields.io/badge/reqwest-0.12-2D7FF9">
    <img alt="Sled" src="https://img.shields.io/badge/sled-0.34-445566">
    <img alt="Status Alpha" src="https://img.shields.io/badge/status-alpha-6B7280">
  </p>
  <p>
    <img alt="Linux" src="https://img.shields.io/badge/linux-supported-1E293B">
    <img alt="macOS" src="https://img.shields.io/badge/macos-supported-1E293B">
    <img alt="Windows" src="https://img.shields.io/badge/windows-supported-1E293B">
  </p>
</div>

## Overview
Swift Launcher is a desktop Minecraft launcher focused on clean UX, fast startup, and reliable downloads. It manages instances, supports modern mod loaders, and provides built-in authentication flows.

## Philosophy
- Fast, reliable downloads with strong progress tracking
- Clear, minimal UI with easy-to-reach actions
- Instance isolation with explicit per-instance settings
- Observable launches with live logs and crash report access

## Features
- <img alt="Instances" src="https://api.iconify.design/simple-icons:minecraft.svg?color=%236B7280" height="14"> Instance management with import/export and per-instance settings
- <img alt="Loaders" src="https://api.iconify.design/simple-icons:modrinth.svg?color=%236B7280" height="14"> Vanilla, Fabric, and Quilt loader support
- <img alt="Mods" src="https://api.iconify.design/simple-icons:curseforge.svg?color=%236B7280" height="14"> Mod management with Modrinth search/install and local jar import
- <img alt="Auth" src="https://api.iconify.design/simple-icons:microsoft.svg?color=%236B7280" height="14"> Microsoft, Ely.by, and LittleSkin authentication
- <img alt="Java" src="https://api.iconify.design/simple-icons:openjdk.svg?color=%236B7280" height="14"> Managed Java downloads (8/17/21) and custom Java path support
- <img alt="Logs" src="https://api.iconify.design/simple-icons:logstash.svg?color=%236B7280" height="14"> Launch logs and crash report access

## Mod Loader Support
- Vanilla
- Fabric
- Quilt
- Forge/NeoForge: not wired yet

## Authentication
- Microsoft device flow
- Ely.by
- LittleSkin

## Logs & Diagnostics
- Live launch log stream in the instance detail view
- One-click log copy for sharing or debugging
- Crash report folder access

## Getting Started
### Requirements
- Rust (2021 edition)
- Java for launching instances (can be downloaded from Settings)

### Build
```bash
cargo build
```

### Run
```bash
cargo run
```

### Test
```bash
cargo test
```

## Configuration
Logging uses `tracing_subscriber` and honors `RUST_LOG`. Default filter is:
```
swift_launcher=info,warn
```

## Project Layout
- `src/main.rs`: application entrypoint
- `src/app.rs`: app state + update loop
- `src/screens/`: UI screens
- `src/instances/`: install/launch + instance data
- `src/download/`: downloader pipeline
- `src/theme.rs`: shared styling

## Notes
- Forge/NeoForge installers are not wired yet.

## Contributing
Issues and PRs are welcome. If you are making UI changes, include screenshots where possible.
