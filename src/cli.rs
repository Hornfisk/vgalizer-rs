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
