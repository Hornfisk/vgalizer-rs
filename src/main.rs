mod app;
mod audio;
mod colors;
mod config;
mod effects;
mod gpu;
mod input;
mod overlay;
mod postprocess;
mod text;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "vgalizer", about = "Audio-reactive DJ visualizer", version)]
pub struct Cli {
    /// DJ name to display
    #[arg(short, long)]
    pub name: Option<String>,

    /// Audio input device name (substring match)
    #[arg(short, long)]
    pub audio_device: Option<String>,

    /// Path to config file
    #[arg(short, long, default_value = "config.json")]
    pub config: String,

    /// List available audio input devices and exit
    #[arg(long)]
    pub list_audio: bool,

    /// Run windowed (not fullscreen)
    #[arg(short, long)]
    pub windowed: bool,

    /// Resolution WxH (e.g. 1920x1080)
    #[arg(short, long)]
    pub resolution: Option<String>,
}

fn main() {
    env_logger::init();
    let cli = Cli::parse();

    if cli.list_audio {
        println!("Available audio input devices:");
        for (i, name) in audio::capture::list_input_devices() {
            println!("  [{}] {}", i, name);
        }
        return;
    }

    let config = config::load(&cli.config, &cli);
    app::run(config);
}
