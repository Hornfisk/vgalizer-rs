pub mod schema;
pub mod watcher;

pub use schema::Config;
pub use watcher::ConfigWatcher;

use crate::cli::Cli;

pub fn load(path: &str, cli: &Cli) -> Config {
    let mut config = load_file(path);

    // CLI overrides
    if let Some(name) = &cli.name {
        config.dj_name = name.clone();
    }
    if let Some(dev) = &cli.audio_device {
        config.audio_device = Some(dev.clone());
    }
    if cli.windowed {
        config.fullscreen = false;
    }
    if let Some(res) = &cli.resolution {
        if let Some((w, h)) = parse_resolution(res) {
            config.resolution = Some((w, h));
        }
    }

    config
}

fn load_file(path: &str) -> Config {
    // Try user-specified path, then XDG config dir, then embedded default
    let content = std::fs::read_to_string(path)
        .or_else(|_| {
            let xdg = dirs_config();
            std::fs::read_to_string(xdg)
        })
        .unwrap_or_else(|_| include_str!("../../config.json").to_string());

    serde_json::from_str(&content).unwrap_or_else(|e| {
        log::warn!("Failed to parse config: {}. Using defaults.", e);
        Config::default()
    })
}

fn dirs_config() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{}/.config/vgalizer/config.json", home)
}

fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().ok()?;
        let h = parts[1].parse().ok()?;
        Some((w, h))
    } else {
        None
    }
}
