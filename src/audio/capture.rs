use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use std::sync::Arc;

use super::analysis::AudioAnalyzer;
use super::state::{AtomicAudioState, N_BANDS};

pub fn list_input_devices() -> Vec<(usize, String)> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices
            .enumerate()
            .map(|(i, d)| (i, d.name().unwrap_or_else(|_| format!("device-{}", i))))
            .collect(),
        Err(e) => {
            log::error!("Failed to enumerate audio devices: {}", e);
            vec![]
        }
    }
}

pub fn start_capture(
    device_name: Option<&str>,
    audio_state: Arc<AtomicAudioState>,
) -> Result<Stream, String> {
    let host = cpal::default_host();

    let device = match device_name {
        Some(name) => find_device(&host, name)?,
        None => host
            .default_input_device()
            .ok_or_else(|| "No default audio input device found".to_string())?,
    };

    log::info!(
        "Audio device: {}",
        device.name().unwrap_or_default()
    );

    let supported_config = device
        .default_input_config()
        .map_err(|e| format!("No supported input config: {}", e))?;

    let config: StreamConfig = supported_config.clone().into();
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0;

    let mut analyzer = AudioAnalyzer::new(sample_rate);

    let stream = device
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
        .map_err(|e| format!("Failed to build audio stream: {}", e))?;

    stream
        .play()
        .map_err(|e| format!("Failed to start audio stream: {}", e))?;

    Ok(stream)
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
