use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use std::sync::Arc;

use super::analysis::AudioAnalyzer;
use super::state::AtomicAudioState;

/// Holds a live capture — either a cpal ALSA stream or a parec subprocess.
/// Drop stops the capture.
pub enum AudioStreamHandle {
    Cpal(Stream),
    Pa(PaCapture),
}

/// A running `parec` subprocess that feeds audio into the analyzer thread.
pub struct PaCapture {
    child: std::process::Child,
    /// Reader thread exits when parec stdout closes (i.e. after kill()).
    _thread: std::thread::JoinHandle<()>,
}

impl Drop for PaCapture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        // Thread unblocks on its own once stdout closes.
    }
}

/// Run `f` with stderr temporarily redirected to /dev/null.
/// Used to silence ALSA's noisy lib-level chatter during device probing
/// (dmix/dsnoop/oss plugins complaining about unsupported stream directions).
/// These messages are harmless but spam the terminal at startup.
fn with_stderr_silenced<R, F: FnOnce() -> R>(f: F) -> R {
    // SAFETY: we dup FD 2, replace it temporarily, then restore. All calls
    // are wrapped in `unsafe` per libc conventions. If any libc call fails
    // we fall back to running `f` without redirection.
    unsafe {
        let saved = libc::dup(libc::STDERR_FILENO);
        if saved < 0 {
            return f();
        }
        let devnull_path = b"/dev/null\0".as_ptr() as *const libc::c_char;
        let devnull = libc::open(devnull_path, libc::O_WRONLY);
        if devnull < 0 {
            libc::close(saved);
            return f();
        }
        libc::dup2(devnull, libc::STDERR_FILENO);
        libc::close(devnull);

        let result = f();

        libc::dup2(saved, libc::STDERR_FILENO);
        libc::close(saved);
        result
    }
}

pub fn list_input_devices() -> Vec<(usize, String)> {
    let host = cpal::default_host();
    // Silence ALSA's noisy startup chatter during probing.
    let probe_result = with_stderr_silenced(|| {
        host.input_devices().map(|devs| {
            devs.enumerate()
                .map(|(i, d)| (i, d.name().unwrap_or_else(|_| format!("device-{}", i))))
                .collect::<Vec<_>>()
        })
    });
    match probe_result {
        Ok(devices) => devices,
        Err(e) => {
            log::error!("Failed to enumerate audio devices: {}", e);
            vec![]
        }
    }
}

/// ALSA devices suitable for the picker (dangerous/useless plugins removed),
/// followed by PulseAudio/PipeWire monitor sources discovered via `pactl`.
///
/// Monitor source entries are encoded as:
///   `"PA:<source_name>"` — source is currently RUNNING (has audio)
///   `"pa:<source_name>"` — source is SUSPENDED/IDLE
///
/// `start_capture` understands both prefixes and routes them through `parec`.
pub fn list_input_devices_for_picker() -> Vec<(usize, String)> {
    const SKIP: &[&str] = &[
        "jack", "lavrate", "samplerate", "speexrate",
        "speex", "upmix", "vdownmix",
    ];
    let mut result: Vec<(usize, String)> = list_input_devices()
        .into_iter()
        .filter(|(_, name)| !SKIP.contains(&name.as_str()))
        .collect();

    let offset = result.len();
    for (i, entry) in list_pa_monitor_sources().into_iter().enumerate() {
        result.push((offset + i, entry));
    }
    result
}

/// Queries `pactl list short sources` and returns monitor sources.
/// Prefix is uppercase `"PA:"` if the source is RUNNING, lowercase `"pa:"` otherwise.
fn list_pa_monitor_sources() -> Vec<String> {
    let output = match std::process::Command::new("pactl")
        .args(["list", "short", "sources"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            // Format: "<idx>\t<name>\t<driver>\t<format>\t<state>"
            let mut cols = line.splitn(5, '\t');
            let _idx = cols.next()?;
            let name = cols.next()?;
            if !name.contains(".monitor") {
                return None;
            }
            let state = cols.nth(2)?.trim(); // skip driver + format, take state
            if state == "RUNNING" {
                Some(format!("PA:{}", name))
            } else {
                Some(format!("pa:{}", name))
            }
        })
        .collect()
}

/// Starts audio capture.
///
/// `device_name` may be:
/// - `None`                → use cpal default input device
/// - `Some("PA:<src>")`    → capture via `parec --device=<src>` (PipeWire/PulseAudio)
/// - `Some("pa:<src>")`    → same as PA: prefix (case-insensitive convention)
/// - `Some("<alsa>")`      → open the named ALSA input device (substring match)
pub fn start_capture(
    device_name: Option<&str>,
    audio_state: Arc<AtomicAudioState>,
) -> Result<AudioStreamHandle, String> {
    // PA monitor sources go through parec, not cpal.
    if let Some(name) = device_name {
        if let Some(pa_src) = name.strip_prefix("PA:").or_else(|| name.strip_prefix("pa:")) {
            return start_parec_capture(pa_src, audio_state);
        }
    }

    // ALSA / default path via cpal. Silence ALSA's noisy probing chatter
    // during device resolution, stream construction, and start.
    let host = cpal::default_host();

    let device = with_stderr_silenced(|| match device_name {
        Some(name) => find_device(&host, name),
        None => host
            .default_input_device()
            .ok_or_else(|| "No default audio input device found".to_string()),
    })?;

    log::info!("Audio device: {}", device.name().unwrap_or_default());

    let supported_config = with_stderr_silenced(|| {
        device
            .default_input_config()
            .map_err(|e| format!("No supported input config: {}", e))
    })?;

    let config: StreamConfig = supported_config.clone().into();
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0;

    let mut analyzer = AudioAnalyzer::new(sample_rate);

    let stream = with_stderr_silenced(|| {
        device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let (level, bands) = analyzer.process(data, channels);
                    audio_state.store_level(level);
                    audio_state.store_bands(&bands);
                },
                |err| log::error!("Audio stream error: {}", err),
                None,
            )
            .map_err(|e| format!("Failed to build audio stream: {}", e))
    })?;

    with_stderr_silenced(|| {
        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {}", e))
    })?;

    Ok(AudioStreamHandle::Cpal(stream))
}

/// Spawns `parec --device=<source>` and feeds its stdout into the analyzer.
/// This bypasses the ALSA layer entirely and speaks directly to
/// PipeWire's PulseAudio compatibility layer.
fn start_parec_capture(
    source: &str,
    audio_state: Arc<AtomicAudioState>,
) -> Result<AudioStreamHandle, String> {
    let mut child = std::process::Command::new("parec")
        .args([
            "--device",
            source,
            "--format=float32le",
            "--channels=2",
            "--rate=44100",
            "--latency-msec=100",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("parec not available: {}. Install pulseaudio-utils.", e))?;

    let stdout = child.stdout.take().unwrap(); // safe: piped above

    let thread = std::thread::spawn(move || {
        use std::io::Read;
        let mut analyzer = AudioAnalyzer::new(44100);
        // 512 stereo f32 frames = 4096 bytes
        let mut buf = vec![0u8; 4096];
        let mut reader = std::io::BufReader::with_capacity(65536, stdout);
        loop {
            // Fill entire buffer before processing to get consistent chunk sizes.
            let mut filled = 0;
            while filled < buf.len() {
                match reader.read(&mut buf[filled..]) {
                    Ok(0) => return, // parec exited / EOF
                    Ok(n) => filled += n,
                    Err(_) => return,
                }
            }
            let samples: Vec<f32> = buf
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            let (level, bands) = analyzer.process(&samples, 2);
            audio_state.store_level(level);
            audio_state.store_bands(&bands);
        }
    });

    log::info!("Started parec capture from '{}'", source);
    Ok(AudioStreamHandle::Pa(PaCapture {
        child,
        _thread: thread,
    }))
}

fn find_device(host: &cpal::Host, name: &str) -> Result<Device, String> {
    let name_lower = name.to_lowercase();
    host.input_devices()
        .map_err(|e| format!("Device enumeration failed: {}", e))?
        .find(|d| {
            d.name()
                .map(|n| n.to_lowercase().contains(&name_lower))
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            format!(
                "Audio device '{}' not found. Run --list-audio to see available devices.",
                name
            )
        })
}
