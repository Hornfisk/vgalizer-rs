/// Verifies that all WGSL shaders parse and validate successfully.
/// Loads globals.wgsl prepended to each effect shader, matching what the
/// Rust loader does at runtime, then asks wgpu's built-in naga frontend to
/// validate the combined source.  Requires a GPU adapter — the test is
/// skipped gracefully when none is available (e.g. headless CI).
use std::path::Path;

fn read(path: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
    )
    .unwrap_or_else(|e| panic!("Cannot read {path}: {e}"))
}

fn wgpu_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .ok()?;
        Some((device, queue))
    })
}

fn compile_shader(device: &wgpu::Device, label: &str, source: &str) {
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let _module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let err = pollster::block_on(device.pop_error_scope());
    assert!(err.is_none(), "Shader '{label}' failed to compile:\n{err:?}");
}

#[test]
fn effect_shaders_compile() {
    let Some((device, _queue)) = wgpu_device() else {
        eprintln!("No GPU adapter available — skipping shader compile test");
        return;
    };

    let globals = read("shaders/globals.wgsl");
    let vert    = read("shaders/fullscreen.wgsl");

    // Vertex shader (standalone, no globals prefix)
    compile_shader(&device, "fullscreen_vert", &vert);

    let effects = [
        "hyperspace",
        "kaleido",
        "ring_tunnel",
        "warp_grid",
        "morph_geo",
        "spectrum_bars",
        "spectrum_orbit",
        "spectrum_terrain",
        "spectrum_wave",
    ];

    for name in effects {
        let frag = read(&format!("shaders/effects/{name}.wgsl"));
        let combined = format!("{globals}\n{frag}");
        compile_shader(&device, &format!("{name}_frag"), &combined);
    }
}

#[test]
fn post_shaders_compile() {
    let Some((device, _queue)) = wgpu_device() else {
        eprintln!("No GPU adapter available — skipping shader compile test");
        return;
    };

    let post_shaders = [
        "shaders/post/trail.wgsl",
        "shaders/post/mirror.wgsl",
        "shaders/post/rotation.wgsl",
        "shaders/post/strobe.wgsl",
        "shaders/post/glitch.wgsl",
        "shaders/post/vga.wgsl",
        "shaders/post/scanlines.wgsl",
    ];

    for path in post_shaders {
        // Post shaders may or may not exist yet — skip missing ones
        let full = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
        if !full.exists() { continue; }
        let src = std::fs::read_to_string(&full)
            .unwrap_or_else(|e| panic!("Cannot read {path}: {e}"));
        let name = Path::new(path).file_stem().unwrap().to_str().unwrap();
        compile_shader(&device, name, &src);
    }
}
