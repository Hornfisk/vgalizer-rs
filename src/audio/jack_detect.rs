//! HDA jack auto-swap watcher (T6b).
//!
//! Watches the `HDA Intel PCH Mic` input device for `SW_MICROPHONE_INSERT`
//! events and publishes plug/unplug notifications over an mpsc channel.
//! The render loop polls the receiver each frame via `try_recv` and, when
//! a change arrives, tears down the current capture and restarts it on
//! the appropriate device.
//!
//! Why input events and not sysfs polling:
//! - `/dev/input/eventN` blocks until a real state change, so a dedicated
//!   watcher thread sits idle between plug events with zero CPU cost.
//! - sysfs has no canonical jack-state file on pingo's E480 — the HDA
//!   driver only surfaces jacks as input-switch devices.
//! - `natalia` is already in the `input` group on pingo, so the event
//!   node is readable without udev rules or PolicyKit.
//!
//! Device naming: the mic/line-in combo jack registers as
//! `HDA Intel PCH Mic` on this box; verified in
//! `/proc/asound/card0/codec#0` jack dump. The watcher enumerates all
//! `/dev/input/event*` via `evdev::enumerate()` and filters by name so
//! the eventN number never gets hardcoded — there is NO relationship
//! between `inputN` and `eventN` indices on Linux.

use std::sync::mpsc::Sender;
use std::thread;

use evdev::{Device, InputEventKind, SwitchType};

/// One-shot notification that the external mic/line-in jack state changed.
#[derive(Debug, Clone, Copy)]
pub enum JackEvent {
    /// External mic / line-in cable just plugged in.
    MicPlugged,
    /// External mic / line-in cable just unplugged.
    MicUnplugged,
}

/// Name of the HDA jack input device on pingo. Other machines may differ;
/// if you port this to a non-E480 box, dump `evdev::enumerate()` and
/// adjust.
const DEVICE_NAME: &str = "HDA Intel PCH Mic";

/// Locate the HDA mic jack input device by name and return it together
/// with its current plugged state. `None` if no such device exists (e.g.
/// codec with no detect-capable combo jack, or running on non-HDA
/// hardware).
pub fn find_device() -> Option<(Device, bool)> {
    for (_path, dev) in evdev::enumerate() {
        if dev.name() == Some(DEVICE_NAME) {
            // Read initial state so the caller can pick the right device
            // on startup without waiting for a user-initiated plug event.
            let plugged = dev
                .get_switch_state()
                .map(|s| s.contains(SwitchType::SW_MICROPHONE_INSERT))
                .unwrap_or(false);
            return Some((dev, plugged));
        }
    }
    None
}

/// Spawns a blocking reader thread that forwards `SW_MICROPHONE_INSERT`
/// transitions to `tx`. The thread exits silently if the evdev fd goes
/// away (unloaded snd_hda_intel module, suspend/resume edge cases).
///
/// Intentionally does NOT hold the `Device` in `AppState` — ownership
/// moves into the thread so the main thread can't accidentally call
/// blocking `fetch_events` from inside the render loop.
pub fn spawn_watcher(mut dev: Device, tx: Sender<JackEvent>) {
    thread::Builder::new()
        .name("vgalizer-jackdetect".into())
        .spawn(move || loop {
            let events = match dev.fetch_events() {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("jack-detect: fetch_events failed: {}. Thread exiting.", e);
                    return;
                }
            };
            for ev in events {
                if let InputEventKind::Switch(SwitchType::SW_MICROPHONE_INSERT) = ev.kind() {
                    let jack_ev = if ev.value() == 1 {
                        JackEvent::MicPlugged
                    } else {
                        JackEvent::MicUnplugged
                    };
                    log::info!("jack-detect: {:?}", jack_ev);
                    // If the receiver has been dropped (app shutting down)
                    // just let the thread die on the next iteration's
                    // fetch_events/send attempt.
                    if tx.send(jack_ev).is_err() {
                        return;
                    }
                }
            }
        })
        .expect("failed to spawn vgalizer-jackdetect thread");
}
