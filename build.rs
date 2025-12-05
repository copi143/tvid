use std::env;
use std::process::exit;

fn main() {
    let features = env::vars()
        .filter(|(k, _)| k.starts_with("CARGO_FEATURE_"))
        .map(|(k, _)| k[14..].to_lowercase())
        .collect::<Vec<_>>();

    let audio_enabled = features.contains(&"audio".to_string());
    let video_enabled = features.contains(&"video".to_string());

    if !audio_enabled && !video_enabled {
        eprintln!("error: Either feature 'audio' or 'video' must be enabled.");
        exit(1);
    }
}
