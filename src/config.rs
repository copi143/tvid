use anyhow::Result;
use data_classes::derive::*;
use parking_lot::Mutex;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::playlist::PLAYLIST;

#[cfg(windows)]
const CONFIG_DIR: &str = "%LocalAppData%\\tvid";
#[cfg(unix)]
const CONFIG_DIR: &str = "~/.config/tvid";

const CONFIG_FILE: &str = "tvid.toml";
const PLAYLIST_FILE: &str = "playlist.txt";
const PLAYLIST_SUBDIR: &str = "playlists";

const DEFAULT_CONFIG_DATA: &[u8] = include_bytes!("tvid.toml");
const DEFAULT_PLAYLIST_DATA: &[u8] = include_bytes!("playlist.txt");

pub static CONFIG: Mutex<Config> = Mutex::new(Config::new());

static ORIG_CONFIG: Mutex<Config> = Mutex::new(Config::new());
static TOML_SOURCE: Mutex<Option<String>> = Mutex::new(None);

#[data(default, serde)]
pub struct Config {
    /// 音量，范围 0-200
    #[default = 100]
    #[serde(default)]
    pub volume: u32,
    /// 是否循环播放播放列表
    #[default = false]
    #[serde(default)]
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
                    let msg = match locale!() {
                        "zh-cn" => "音量必须在 0 到 200 之间".to_string(),
                        "zh-tw" => "音量必須在 0 到 200 之間".to_string(),
                        "ja-jp" => "音量は0〜200の間でなければなりません".to_string(),
                        "fr-fr" => "Le volume doit être entre 0 et 200".to_string(),
                        "de-de" => "Die Lautstärke muss zwischen 0 und 200 liegen".to_string(),
                        "es-es" => "El volumen debe estar entre 0 y 200".to_string(),
                        _ => "Volume must be between 0 and 200".to_string(),
                    };
                    anyhow::bail!("{}", msg);
                }
            }
            "looping" => {
                let b = value.parse::<bool>()?;
                self.looping = b;
            }
            _ => {
                let tpl = match locale!() {
                    "zh-cn" => "未知的配置项: {}",
                    "zh-tw" => "未知的設定項: {}",
                    "ja-jp" => "不明な設定キー: {}",
                    "fr-fr" => "Clé de configuration inconnue : {}",
                    "de-de" => "Unbekannter Konfigurationsschlüssel: {}",
                    "es-es" => "Clave de configuración desconocida: {}",
                    _ => "Unknown config key: {}",
                };
                let msg = tpl.replace("{}", key);
                anyhow::bail!("{}", msg);
            }
        }
        Ok(())
    }

    pub fn write_to(&self, wr: &mut dyn Write) -> Result<()> {
        let mut src_opt = TOML_SOURCE.lock();
        let src = if let Some(s) = src_opt.take() {
            s
        } else {
            String::from_utf8(DEFAULT_CONFIG_DATA.to_vec())?
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
    let dir = shellexpand::tilde(dir.unwrap_or(CONFIG_DIR)).to_string();

    let path = Path::new(&dir).join(CONFIG_FILE);
    load_config(File::open(path)?)?;

    let path = Path::new(&dir).join(PLAYLIST_FILE);
    load_playlist(File::open(path)?)?;

    Ok(())
}

fn save_config(mut file: File) -> Result<()> {
    CONFIG.lock().write_to(&mut file)?;
    Ok(())
}

fn save_playlist(mut file: File) -> Result<()> {
    file.write_all(DEFAULT_PLAYLIST_DATA)?;
    for item in PLAYLIST.lock().get_items() {
        writeln!(file, "{}", item)?;
    }
    Ok(())
}

pub fn save(dir: Option<&str>) -> Result<()> {
    let dir = shellexpand::tilde(dir.unwrap_or(CONFIG_DIR)).to_string();

    let path = Path::new(&dir).join(CONFIG_FILE);
    save_config(File::create(path)?)?;

    let path = Path::new(&dir).join(PLAYLIST_FILE);
    save_playlist(File::create(path)?)?;

    Ok(())
}

pub fn create_if_not_exists(dir: Option<&str>) -> Result<()> {
    let dir = shellexpand::tilde(dir.unwrap_or(CONFIG_DIR)).to_string();
    let dir = Path::new(&dir);
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
    }

    let playlist_dir = dir.join(PLAYLIST_SUBDIR);
    if !playlist_dir.exists() {
        std::fs::create_dir_all(playlist_dir)?;
    }

    let path = dir.join(CONFIG_FILE);
    if !path.exists() {
        let mut file = File::create(path)?;
        file.write_all(DEFAULT_CONFIG_DATA)?;
    }

    let path = dir.join(PLAYLIST_FILE);
    if !path.exists() {
        let mut file = File::create(path)?;
        file.write_all(DEFAULT_PLAYLIST_DATA)?;
    }

    Ok(())
}

#[cfg(feature = "ssh")]
pub fn load_or_create_hostkeys(dir: Option<&str>) -> Result<Vec<russh::keys::PrivateKey>> {
    use anyhow::bail;
    use russh::keys::signature::rand_core::OsRng;
    use russh::keys::ssh_key::private::{Ed25519Keypair, RsaKeypair};

    const SSH_HOSTKEY_RSA_FILE: &str = "hostkey_rsa";
    const SSH_HOSTKEY_ED25519_FILE: &str = "hostkey_ed25519";

    let dir = shellexpand::tilde(dir.unwrap_or(CONFIG_DIR)).to_string();
    let dir = Path::new(&dir);

    let keypath_rsa = dir.join(SSH_HOSTKEY_RSA_FILE);
    let keypath_ed25519 = dir.join(SSH_HOSTKEY_ED25519_FILE);

    let hostkey_rsa = if let Ok(k) = russh::keys::load_secret_key(&keypath_rsa, None) {
        k
    } else if let Ok(mut f) = std::fs::File::create(&keypath_rsa) {
        let kp = RsaKeypair::random(&mut OsRng, 2048)?.into();
        russh::keys::encode_pkcs8_pem(&kp, &mut f)?;
        kp
    } else {
        bail!("failed to load or create host key at {keypath_rsa:?}");
    };

    let hostkey_ed25519 = if let Ok(k) = russh::keys::load_secret_key(&keypath_ed25519, None) {
        k
    } else if let Ok(mut f) = std::fs::File::create(&keypath_ed25519) {
        let kp = Ed25519Keypair::random(&mut OsRng).into();
        russh::keys::encode_pkcs8_pem(&kp, &mut f)?;
        kp
    } else {
        bail!("failed to load or create host key at {keypath_ed25519:?}");
    };

    Ok(vec![hostkey_rsa, hostkey_ed25519])
}
