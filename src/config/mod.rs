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
    // Base layer: user-supplied `-c` path, falling back to the embedded
    // default shipped in the repo. This provides the "shape" of the config
    // (visuals, audio defaults, mirror pool, etc.).
    let base_str = std::fs::read_to_string(path)
        .unwrap_or_else(|_| include_str!("../../config.json").to_string());
    let mut base: serde_json::Value = serde_json::from_str(&base_str)
        .unwrap_or_else(|e| {
            log::warn!("Failed to parse base config: {}. Using empty.", e);
            serde_json::json!({})
        });

    // Overlay: the XDG file holds user state that should survive code
    // iterations — edited params, DJ name, audio device, scene duration,
    // disabled-effects deny list. Merging shallowly means new base fields
    // still flow through on rebuilds, and stale XDG fields are tolerated.
    if let Ok(xdg_str) = std::fs::read_to_string(dirs_config()) {
        if let Ok(xdg_val) = serde_json::from_str::<serde_json::Value>(&xdg_str) {
            if let (Some(base_obj), Some(xdg_obj)) = (base.as_object_mut(), xdg_val.as_object()) {
                for (k, v) in xdg_obj {
                    // Special-case fx_params: do a per-effect merge so that
                    // values from either layer win individually. The base
                    // provides baseline params for new effects; the XDG
                    // layer holds the user's tweaks on top.
                    if k == "fx_params" {
                        if let Some(v_obj) = v.as_object() {
                            let entry = base_obj
                                .entry("fx_params".to_string())
                                .or_insert_with(|| serde_json::json!({}));
                            if let Some(entry_obj) = entry.as_object_mut() {
                                for (fx, params) in v_obj {
                                    entry_obj.insert(fx.clone(), params.clone());
                                }
                            }
                        }
                        continue;
                    }
                    base_obj.insert(k.clone(), v.clone());
                }
            }
        }
    }

    serde_json::from_value(base).unwrap_or_else(|e| {
        log::warn!("Failed to finalize config: {}. Using defaults.", e);
        Config::default()
    })
}

pub fn dirs_config() -> String {
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
    write_string_field_to_path(path, "audio_device", device_name)
}

/// Persist a new DJ name to the XDG config file, atomically.
pub fn write_dj_name(dj_name: &str) -> std::io::Result<()> {
    write_string_field_to_path(&dirs_config(), "dj_name", dj_name)
}

/// Persist a single per-effect float parameter to the given config file
/// (typically the repo `config.json` so the value travels via git).
/// Reads existing JSON, sets `fx_params[effect][name] = value`, atomic write.
pub fn write_fx_param(path: &str, effect: &str, name: &str, value: f32) -> std::io::Result<()> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }

    let mut json: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    if !json.get("fx_params").map(|v| v.is_object()).unwrap_or(false) {
        json["fx_params"] = serde_json::json!({});
    }
    if !json["fx_params"].get(effect).map(|v| v.is_object()).unwrap_or(false) {
        json["fx_params"][effect] = serde_json::json!({});
    }
    json["fx_params"][effect][name] =
        serde_json::Value::from(((value * 10000.0).round() / 10000.0) as f64);

    let pretty = serde_json::to_string_pretty(&json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let tmp = format!("{}.tmp", path);
    std::fs::write(&tmp, &pretty)?;
    std::fs::rename(&tmp, path)
}

/// Persist a batch of top-level JSON-valued fields to the XDG config in a
/// single atomic write. Used by the G global-settings menu so all eight
/// knobs land in one tmp→rename instead of eight.
pub fn write_xdg_fields(updates: &[(&str, serde_json::Value)]) -> std::io::Result<()> {
    let path = dirs_config();
    if let Some(dir) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(dir)?;
    }

    let mut json: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    for (k, v) in updates {
        json[*k] = v.clone();
    }

    let pretty = serde_json::to_string_pretty(&json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let tmp = format!("{}.tmp", path);
    std::fs::write(&tmp, &pretty)?;
    std::fs::rename(&tmp, &path)
}

/// Persist the autopilot scene duration (in seconds) to the XDG config.
pub fn write_scene_duration(secs: f64) -> std::io::Result<()> {
    let path = dirs_config();
    write_json_field(&path, "scene_duration", serde_json::json!(secs))
}

/// Persist the disabled-effects deny list to the XDG config. Pass `None`
/// (or an empty slice) to clear the field (= all effects enabled).
///
/// Using a deny list means new effects added in code updates are always
/// enabled by default — the user only has to remember what they turned off.
pub fn write_disabled_effects(disabled: Option<&[String]>) -> std::io::Result<()> {
    let path = dirs_config();
    let value = match disabled {
        Some(list) if !list.is_empty() => serde_json::Value::Array(
            list.iter().map(|s| serde_json::Value::String(s.clone())).collect(),
        ),
        _ => serde_json::Value::Null,
    };
    write_json_field(&path, "disabled_effects", value)
}

/// Generic helper: set a single top-level JSON-valued field in the config,
/// preserving all other fields, atomic tmp→rename.
fn write_json_field(path: &str, key: &str, value: serde_json::Value) -> std::io::Result<()> {
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }

    let mut json: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    json[key] = value;

    let pretty = serde_json::to_string_pretty(&json)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let tmp = format!("{}.tmp", path);
    std::fs::write(&tmp, &pretty)?;
    std::fs::rename(&tmp, path)
}

/// Generic helper: set a single top-level string field in the JSON config,
/// preserving all other fields, atomic tmp→rename.
fn write_string_field_to_path(path: &str, key: &str, value: &str) -> std::io::Result<()> {
    // Ensure parent directory exists
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }

    // Load existing JSON or start fresh
    let mut json: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    json[key] = serde_json::Value::String(value.to_string());

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
