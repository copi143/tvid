use anyhow::Result;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::playlist::PLAYLIST;

#[cfg(windows)]
const DEFAULT_CONFIG_DIR: &str = "%LocalAppData%\\tvid";
#[cfg(unix)]
const DEFAULT_CONFIG_DIR: &str = "~/.config/tvid";

const DEFAULT_CONFIG_FILE: &str = "tvid.toml";
const DEFAULT_PLAYLIST_FILE: &str = "playlist.txt";
const DEFAULT_PLAYLIST_SUBDIR: &str = "playlists";

const DEFAULT_CONFIG_FILE_DATA: &[u8] = include_bytes!("tvid.toml");
const DEFAULT_PLAYLIST_FILE_DATA: &[u8] = include_bytes!("playlist.txt");

pub static CONFIG: Mutex<Config> = Mutex::new(Config::new());

static ORIG_CONFIG: Mutex<Config> = Mutex::new(Config::new());
static TOML_SOURCE: Mutex<Option<String>> = Mutex::new(None);

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    /// 音量，范围 0-200
    pub volume: u32,
    /// 是否循环播放播放列表
    pub looping: bool,
}

impl Config {
    pub const fn new() -> Self {
        Self {
            volume: 100,
            looping: false,
        }
    }

    pub fn set_entry(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "volume" => {
                let v = value.parse::<u32>()?;
                if v <= 200 {
                    self.volume = v;
                } else {
                    anyhow::bail!("Volume must be between 0 and 200");
                }
            }
            "looping" => {
                let b = value.parse::<bool>()?;
                self.looping = b;
            }
            _ => {
                anyhow::bail!("Unknown config key: {}", key);
            }
        }
        Ok(())
    }

    pub fn write_to(&self, wr: &mut dyn Write) -> Result<()> {
        let mut src_opt = TOML_SOURCE.lock();
        let src = if let Some(s) = src_opt.take() {
            s
        } else {
            String::from_utf8(DEFAULT_CONFIG_FILE_DATA.to_vec())?
        };

        let mut doc: toml_edit::DocumentMut = src.parse()?;
        for (k, v) in toml_edit::ser::to_document(self)?.iter() {
            doc[k] = v.clone();
        }

        let out = doc.to_string();
        wr.write_all(out.as_bytes())?;

        *src_opt = Some(out);

        Ok(())
    }
}

fn load_config(file: File) -> Result<()> {
    let mut s = String::new();
    let mut f = file;
    f.read_to_string(&mut s)?;

    // 保存文档源以便后续写入保持注释
    *TOML_SOURCE.lock() = Some(s.clone());

    // 使用 toml_edit 的 serde 支持反序列化整个文档到 Config
    let cfg: Config = toml_edit::de::from_str(&s)?;
    *CONFIG.lock() = cfg;

    Ok(())
}

fn load_playlist(mut file: File) -> Result<()> {
    let mut s = String::new();
    file.read_to_string(&mut s)?;
    let playlist = s
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
    load_playlist(File::open(path)?)?;

    Ok(())
}

fn save_config(mut file: File) -> Result<()> {
    CONFIG.lock().write_to(&mut file)?;
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

pub fn create_if_not_exists(dir: Option<&str>) -> Result<()> {
    let dir = shellexpand::tilde(dir.unwrap_or(DEFAULT_CONFIG_DIR)).to_string();
    let dir = Path::new(&dir);
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }

    let playlist_dir = dir.join(DEFAULT_PLAYLIST_SUBDIR);
    if !playlist_dir.exists() {
        std::fs::create_dir_all(playlist_dir)?;
    }

    let path = dir.join(DEFAULT_CONFIG_FILE);
    if !path.exists() {
        let mut file = File::create(path)?;
        file.write_all(DEFAULT_CONFIG_FILE_DATA)?;
    }

    let path = dir.join(DEFAULT_PLAYLIST_FILE);
    if !path.exists() {
        let mut file = File::create(path)?;
        file.write_all(DEFAULT_PLAYLIST_FILE_DATA)?;
    }

    Ok(())
}
