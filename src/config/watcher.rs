use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::Config;

pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<Event>>,
    config_path: PathBuf,
    last_reload: Instant,
    debounce: Duration,
}

impl ConfigWatcher {
    pub fn new(path: &str) -> Option<Self> {
        let config_path = PathBuf::from(path);
        if !config_path.exists() {
            return None;
        }
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(tx).ok()?;
        watcher
            .watch(Path::new(path), RecursiveMode::NonRecursive)
            .ok()?;
        Some(Self {
            _watcher: watcher,
            rx,
            config_path,
            last_reload: Instant::now(),
            debounce: Duration::from_millis(100),
        })
    }

    /// Returns Some(Config) if the file has changed and parsed successfully.
    pub fn poll(&mut self) -> Option<Config> {
        let mut changed = false;
        while let Ok(Ok(_event)) = self.rx.try_recv() {
            changed = true;
        }
        if changed && self.last_reload.elapsed() > self.debounce {
            self.last_reload = Instant::now();
            let content = std::fs::read_to_string(&self.config_path).ok()?;
            match serde_json::from_str(&content) {
                Ok(cfg) => return Some(cfg),
                Err(e) => log::warn!("Config reload parse error: {}", e),
            }
        }
        None
    }
}
