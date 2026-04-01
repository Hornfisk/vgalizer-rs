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

/// Writes (or overwrites) the `audio_device` key in the XDG config file,
/// creating the file and parent directories as needed.
/// Uses an atomic rename so the file is never partially written.
pub fn write_audio_device(device_name: &str) -> std::io::Result<()> {
    write_audio_device_to_path(&dirs_config(), device_name)
}

pub fn write_audio_device_to_path(path: &str, device_name: &str) -> std::io::Result<()> {
    // Ensure parent directory exists
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }

    // Load existing JSON or start fresh
    let mut json: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    json["audio_device"] = serde_json::Value::String(device_name.to_string());

    let pretty = serde_json::to_string_pretty(&json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Atomic write: tmp → rename
    let tmp = format!("{}.tmp", path);
    std::fs::write(&tmp, &pretty)?;
    std::fs::rename(&tmp, path)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_audio_device_creates_new_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("vgalizer_test_write_new.json");
        let path_str = path.to_str().unwrap();

        // Remove if leftover from a previous run
        let _ = std::fs::remove_file(path_str);

        write_audio_device_to_path(path_str, "USB Audio Interface").unwrap();

        let content = std::fs::read_to_string(path_str).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["audio_device"], "USB Audio Interface");

        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn write_audio_device_updates_existing_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("vgalizer_test_write_update.json");
        let path_str = path.to_str().unwrap();

        // Pre-populate with some existing fields
        std::fs::write(path_str, r#"{"dj_name":"Test DJ","audio_device":"old device"}"#).unwrap();

        write_audio_device_to_path(path_str, "New Device").unwrap();

        let content = std::fs::read_to_string(path_str).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["audio_device"], "New Device");
        // Other fields must be preserved
        assert_eq!(json["dj_name"], "Test DJ");

        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn write_audio_device_creates_parent_dirs() {
        let dir = std::env::temp_dir().join("vgalizer_test_nested_dir");
        let path = dir.join("config.json");
        let path_str = path.to_str().unwrap();

        // Remove any leftover
        let _ = std::fs::remove_dir_all(&dir);

        write_audio_device_to_path(path_str, "Test Device").unwrap();

        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
