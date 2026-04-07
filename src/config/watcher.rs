use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{dirs_config, load_merged, Config};

/// Watches the user's XDG config file for external edits (vje, vim, another
/// machine pushing via sync, etc.) and produces fresh fully-merged `Config`
/// values on demand.
///
/// We watch the *parent directory* of the XDG file rather than the file
/// itself: editors like vim save by writing a new inode and renaming over
/// the old one, which silently detaches an inotify file watch. Watching the
/// directory and filtering by filename survives the rename.
///
/// On reload we re-run the full base+XDG merge (`load_merged`) so changes to
/// either layer flow in identically to startup.
pub struct ConfigWatcher {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<notify::Result<Event>>,
    seed_path: String,
    xdg_path: PathBuf,
    target_filename: OsString,
    last_reload: Instant,
    debounce: Duration,
}

impl ConfigWatcher {
    /// `seed_path` is the repo's seed `config.json` (the CLI `--config`
    /// path). It's never modified at runtime but is needed by `load_merged`
    /// as the base layer.
    pub fn new(seed_path: &str) -> Option<Self> {
        let xdg_path = PathBuf::from(dirs_config());

        // Make sure the XDG file exists so the directory watch always has a
        // target to filter on. First-run users won't have it yet.
        if let Some(parent) = xdg_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if !xdg_path.exists() {
            let _ = std::fs::write(&xdg_path, "{}\n");
        }

        let parent = xdg_path.parent()?.to_path_buf();
        let target_filename = xdg_path.file_name()?.to_os_string();

        let (tx, rx) = mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => {
                log::warn!("ConfigWatcher: failed to create notify watcher: {}", e);
                return None;
            }
        };
        if let Err(e) = watcher.watch(Path::new(&parent), RecursiveMode::NonRecursive) {
            log::warn!("ConfigWatcher: failed to watch {:?}: {}", parent, e);
            return None;
        }
        log::info!(
            "ConfigWatcher: watching dir {:?} for file {:?} (seed: {})",
            parent, target_filename, seed_path
        );

        Some(Self {
            _watcher: watcher,
            rx,
            seed_path: seed_path.to_string(),
            xdg_path,
            target_filename,
            last_reload: Instant::now(),
            debounce: Duration::from_millis(100),
        })
    }

    /// Returns Some(Config) if the XDG file has changed since the last poll
    /// and the merged config parsed successfully.
    pub fn poll(&mut self) -> Option<Config> {
        let mut changed = false;
        while let Ok(res) = self.rx.try_recv() {
            match res {
                Ok(event) => {
                    if event
                        .paths
                        .iter()
                        .any(|p| p.file_name() == Some(self.target_filename.as_os_str()))
                    {
                        changed = true;
                    }
                }
                Err(e) => log::warn!("ConfigWatcher: notify error: {}", e),
            }
        }
        if !changed || self.last_reload.elapsed() <= self.debounce {
            return None;
        }
        self.last_reload = Instant::now();

        // Verify the file still exists (rename gap can be transient).
        if !self.xdg_path.exists() {
            return None;
        }
        log::info!("ConfigWatcher: reloading merged config");
        Some(load_merged(&self.seed_path))
    }
}
