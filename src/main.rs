use vgalizer::audio;
use vgalizer::app;
use vgalizer::config;
use vgalizer::cli::Cli;
use clap::Parser;

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

    let config_path = cli.config.clone();
    let config = config::load(&config_path, &cli);
    app::run(config, config_path);
}
