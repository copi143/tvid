use anyhow::Result;
use std::{
    fs::File,
    io::Write,
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
};

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
        let key = parts[0].trim();
        let value = parts[1].trim();
        match key {
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

pub fn save_playlist(mut file: File) -> Result<()> {
    file.write_all(DEFAULT_PLAYLIST_FILE_DATA)?;
    for item in PLAYLIST.lock().get_items() {
        writeln!(file, "{}", item)?;
    }
    Ok(())
}

pub fn save(dir: Option<&str>) -> Result<()> {
    let dir = shellexpand::tilde(dir.unwrap_or(DEFAULT_CONFIG_DIR)).to_string();
    // let path = Path::new(&dir).join(DEFAULT_CONFIG_FILE);
    // let mut file = File::create(path)?;
    // writeln!(file, "volume = {}", VOLUME.load(Ordering::Relaxed))?;

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
