use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::playlist::PLAYLIST;

const DEFAULT_CONFIG_DIR: &str = "~/.config/tvid";
const DEFAULT_CONFIG_FILE: &str = "tvid.cfg";
const DEFAULT_PLAYLIST_FILE: &str = "playlist.txt";

static VOLUME: AtomicU32 = AtomicU32::new(100);

fn load_config(file: File) -> Result<()> {
    let config = std::io::read_to_string(file)?;
    let config = config
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'));
    for line in config {
        let parts = line.splitn(2, '=').collect::<Vec<_>>();
        if parts.len() != 2 {
            send_error!("Invalid config line: {}", line);
        }
        let (key, value) = (parts[0].trim(), parts[1].trim());
        match key {
            "volume" => match value.parse::<u32>() {
                Ok(v) if v <= 200 => VOLUME.store(v, Ordering::Relaxed),
                _ => send_error!("Invalid volume value: {}", value),
            },
            _ => send_error!("Unknown config key: {}", key),
        }
    }
    Ok(())
}

fn load_playlist(file: &File) -> Result<()> {
    let playlist = std::io::read_to_string(file)?
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect::<Vec<_>>();
    PLAYLIST.lock().clear().extend(playlist);
    Ok(())
}

pub fn load(dir: Option<&str>) -> Result<()> {
    let dir = shellexpand::tilde(dir.unwrap_or(DEFAULT_CONFIG_DIR)).to_string();

    let path = Path::new(&dir).join(DEFAULT_CONFIG_FILE);
    load_config(File::open(path)?)?;

    let path = Path::new(&dir).join(DEFAULT_PLAYLIST_FILE);
    load_playlist(&File::open(path)?)?;

    Ok(())
}

fn save_config(mut file: File) -> Result<()> {
    writeln!(file, "volume = {}", VOLUME.load(Ordering::Relaxed))?;
    Ok(())
}

fn save_playlist(mut file: File) -> Result<()> {
    file.write_all(DEFAULT_PLAYLIST_FILE_DATA)?;
    for item in PLAYLIST.lock().get_items() {
        writeln!(file, "{}", item)?;
    }
    Ok(())
}

pub fn save(dir: Option<&str>) -> Result<()> {
    let dir = shellexpand::tilde(dir.unwrap_or(DEFAULT_CONFIG_DIR)).to_string();

    let path = Path::new(&dir).join(DEFAULT_CONFIG_FILE);
    save_config(File::create(path)?)?;

    let path = Path::new(&dir).join(DEFAULT_PLAYLIST_FILE);
    save_playlist(File::create(path)?)?;

    Ok(())
}

const DEFAULT_CONFIG_FILE_DATA: &[u8] = include_bytes!("tvid.cfg");
const DEFAULT_PLAYLIST_FILE_DATA: &[u8] = include_bytes!("playlist.txt");

pub fn create_if_not_exists(dir: Option<&str>) -> Result<()> {
    let dir = shellexpand::tilde(dir.unwrap_or(DEFAULT_CONFIG_DIR)).to_string();
    if !Path::new(&dir).exists() {
        std::fs::create_dir_all(&dir)?;
    }

    let path = Path::new(&dir).join(DEFAULT_CONFIG_FILE);
    if !path.exists() {
        let mut file = File::create(path)?;
        file.write_all(DEFAULT_CONFIG_FILE_DATA)?;
    }

    let path = Path::new(&dir).join(DEFAULT_PLAYLIST_FILE);
    if !path.exists() {
        let mut file = File::create(path)?;
        file.write_all(DEFAULT_PLAYLIST_FILE_DATA)?;
    }

    Ok(())
}
