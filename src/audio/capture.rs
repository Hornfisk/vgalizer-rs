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
    let pa = list_pa_monitor_sources();
    let pa_was_empty = pa.is_empty();
    for (i, entry) in pa.into_iter().enumerate() {
        result.push((offset + i, entry));
    }
    // On systems without pipewire-pulse `pactl` returns nothing — fall
    // back to native PipeWire enumeration so the picker still has useful
    // entries. Avoid duplicating when both layers are present.
    if pa_was_empty {
        let offset = result.len();
        for (i, entry) in list_pw_nodes().into_iter().enumerate() {
            result.push((offset + i, entry));
        }
    }
    result
}

/// Returns the `node.name` of the first RUNNING Audio/Sink (preferred —
/// pw-cat will record its monitor) or Audio/Source on the native PipeWire
/// graph, queried via `pw-dump`. Used as a pulse-compat-free fallback for
/// `first_running_pa_monitor` on systems without `pipewire-pulse`.
fn first_running_pw_node() -> Option<String> {
    let output = std::process::Command::new("pw-dump").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let arr = value.as_array()?;

    let mut sink: Option<String> = None;
    let mut source: Option<String> = None;
    for obj in arr {
        if obj.get("type").and_then(|t| t.as_str()) != Some("PipeWire:Interface:Node") {
            continue;
        }
        let info = obj.get("info")?;
        if info.get("state").and_then(|s| s.as_str()) != Some("running") {
            continue;
        }
        let props = info.get("props")?;
        let class = props.get("media.class").and_then(|v| v.as_str()).unwrap_or("");
        let name = match props.get("node.name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if class == "Audio/Sink" && sink.is_none() {
            sink = Some(name);
        } else if class == "Audio/Source" && source.is_none() {
            source = Some(name);
        }
    }
    sink.or(source)
}

/// Lists Audio/Sink and Audio/Source nodes via `pw-dump`. Each entry is
/// returned with a `PW:` prefix for RUNNING nodes and `pw:` for others,
/// mirroring the `PA:`/`pa:` convention. Used by the picker on systems
/// without `pipewire-pulse` (where `pactl` is unavailable).
fn list_pw_nodes() -> Vec<String> {
    let output = match std::process::Command::new("pw-dump").output() {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };
    let value: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let arr = match value.as_array() {
        Some(a) => a,
        None => return vec![],
    };
    let mut out = Vec::new();
    for obj in arr {
        if obj.get("type").and_then(|t| t.as_str()) != Some("PipeWire:Interface:Node") {
            continue;
        }
        let info = match obj.get("info") {
            Some(i) => i,
            None => continue,
        };
        let state = info.get("state").and_then(|s| s.as_str()).unwrap_or("");
        let props = match info.get("props") {
            Some(p) => p,
            None => continue,
        };
        let class = props.get("media.class").and_then(|v| v.as_str()).unwrap_or("");
        if class != "Audio/Sink" && class != "Audio/Source" {
            continue;
        }
        let name = match props.get("node.name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };
        let prefix = if state == "running" { "PW" } else { "pw" };
        out.push(format!("{}:{}", prefix, name));
    }
    out
}

/// Returns the name of the first RUNNING PulseAudio/PipeWire monitor source,
/// if any. Used to auto-route "default" capture to whatever is currently
/// playing audio (e.g. the DJ controller's output monitor) instead of
/// whatever cpal's ALSA default resolves to.
fn first_running_pa_monitor() -> Option<String> {
    let output = std::process::Command::new("pactl")
        .args(["list", "short", "sources"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let mut cols = line.splitn(5, '\t');
            let _idx = cols.next()?;
            let name = cols.next()?;
            if !name.contains(".monitor") {
                return None;
            }
            let state = cols.nth(2)?.trim();
            if state == "RUNNING" {
                Some(name.to_string())
            } else {
                None
            }
        })
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
    // Explicit PA monitor sources go through parec (pulse-compat path).
    if let Some(name) = device_name {
        if let Some(pa_src) = name.strip_prefix("PA:").or_else(|| name.strip_prefix("pa:")) {
            return start_parec_capture(pa_src, audio_state);
        }
        // Explicit native-PipeWire nodes go through pw-cat.
        if let Some(pw_node) = name.strip_prefix("PW:").or_else(|| name.strip_prefix("pw:")) {
            return start_pw_cat_capture(pw_node, audio_state);
        }
    }

    // When no device is specified or the user saved "default", the cpal
    // ALSA default rarely lands on anything useful under pipewire-alsa —
    // it usually opens a mic or a silent loopback. Prefer whatever sink
    // monitor / source is currently RUNNING (e.g. the DJ controller
    // playback), so the visualizer reacts out-of-the-box.
    //
    // Try the pulse-compat path first (parec/pactl), then fall back to
    // native PipeWire (pw-cat/pw-dump) for systems without pipewire-pulse.
    let is_default = device_name.map_or(true, |n| n.eq_ignore_ascii_case("default"));
    if is_default {
        if let Some(src) = first_running_pa_monitor() {
            log::info!("Auto-selected running PA monitor source: {}", src);
            return start_parec_capture(&src, audio_state);
        }
        if let Some(node) = first_running_pw_node() {
            log::info!("Auto-selected running PipeWire node: {}", node);
            return start_pw_cat_capture(&node, audio_state);
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
/// Routes through PipeWire's PulseAudio compatibility layer (`pipewire-pulse`).
fn start_parec_capture(
    source: &str,
    audio_state: Arc<AtomicAudioState>,
) -> Result<AudioStreamHandle, String> {
    let mut cmd = std::process::Command::new("parec");
    cmd.args([
        "--device",
        source,
        "--format=float32le",
        "--channels=2",
        "--rate=44100",
        "--latency-msec=100",
    ]);
    let handle = spawn_subprocess_capture(cmd, audio_state)
        .map_err(|e| format!("parec not available: {}. Install pulseaudio-utils.", e))?;
    log::info!("Started parec capture from '{}'", source);
    Ok(handle)
}

/// Spawns `pw-cat --record --target=<node>` and feeds its raw f32 stdout
/// into the analyzer. Native PipeWire path — works on systems without
/// `pipewire-pulse` (no parec/pactl), since `pw-cat` ships with the base
/// `pipewire` package. When `target` names an Audio/Sink node, pw-cat
/// records that sink's monitor; for an Audio/Source node it records the
/// source directly.
fn start_pw_cat_capture(
    target: &str,
    audio_state: Arc<AtomicAudioState>,
) -> Result<AudioStreamHandle, String> {
    let mut cmd = std::process::Command::new("pw-cat");
    cmd.args([
        "--record",
        &format!("--target={}", target),
        "--format=f32",
        "--rate=44100",
        "--channels=2",
        "--latency=100ms",
        "--raw",
        "-",
    ]);
    let handle = spawn_subprocess_capture(cmd, audio_state)
        .map_err(|e| format!("pw-cat not available: {}. Install pipewire.", e))?;
    log::info!("Started pw-cat capture from '{}'", target);
    Ok(handle)
}

/// Spawns a subprocess that writes raw little-endian f32 stereo samples
/// to its stdout, and starts a reader thread that pumps the bytes into
/// `AudioAnalyzer` and the shared `AtomicAudioState`. Used by both the
/// `parec` (PA-compat) and `pw-cat` (native PipeWire) capture paths.
fn spawn_subprocess_capture(
    mut cmd: std::process::Command,
    audio_state: Arc<AtomicAudioState>,
) -> std::io::Result<AudioStreamHandle> {
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = cmd.spawn()?;
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
                    Ok(0) => return, // child exited / EOF
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
