#![allow(clippy::collapsible_if)]
#![allow(clippy::bool_comparison)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::iter_nth_zero)]
#![allow(clippy::len_zero)]
#![allow(clippy::new_without_default)]
#![allow(clippy::len_without_is_empty)]
#![allow(clippy::partialeq_to_none)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::option_map_unit_fn)]

use anyhow::{Context, Result};
use clap::Parser;
use ffmpeg_next as av;
use parking_lot::Mutex;
use std::env;
use std::sync::atomic::Ordering;
use std::sync::{LazyLock, OnceLock};
use std::time::Instant;
use tokio::runtime::Runtime;

use crate::escape::format_link;
use crate::ffmpeg::seek_request_relative;
use crate::render::{COLOR_MODE, FORCEFLUSH_NEXT};
use crate::ui::QUIT_CONFIRMATION;
use crate::{playlist::PLAYLIST, stdin::Key, term::TERM_QUIT};

macro_rules! usemod {
    ($name:ident) => {
        mod $name;
        #[allow(unused)]
        pub use $name::*;
    };
}

#[cfg(feature = "i18n")]
macro_rules! locale {
    () => {
        crate::LOCALE.as_str()
    };
}

#[cfg(not(feature = "i18n"))]
macro_rules! locale {
    () => {
        "en-us"
    };
}

#[allow(unused)]
mod util;

#[macro_use]
mod logging;

#[macro_use]
mod ui;

mod avsync;
mod ffmpeg;
mod playlist;
mod render;
mod statistics;
mod stdin;
mod stdout;
mod term;

#[cfg(feature = "config")]
mod config;

#[cfg(feature = "audio")]
mod audio;

#[cfg(feature = "video")]
mod video;

#[cfg(feature = "subtitle")]
mod subtitle;

mod escape {
    #[cfg(feature = "sixel")]
    usemod!(sixel);
    usemod!(osc8);
    #[cfg(feature = "osc1337")]
    usemod!(osc1337);
}

pub static TOKIO_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    let num_cores = std::thread::available_parallelism().unwrap().get();
    let workers = num_cores.max(4); // 防止 I/O 卡顿
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime")
});

#[cfg(feature = "i18n")]
pub static LOCALE: LazyLock<String> = LazyLock::new(|| {
    sys_locale::get_locale()
        .map(|l| l.to_lowercase())
        .unwrap_or("en-us".to_string())
});

macro_rules! eprintlns {
    ($($fmt:expr $(, $args:expr)*);+ $(;)?) => {{
        $(
            eprintln!($fmt $(, $args)*);
        )+
    }};
}

fn print_no_playlist(program_name: &str) {
    let divider = "-".repeat(term::get_winsize().map(|w| w.col as usize).unwrap_or(80));
    let version = env!("CARGO_PKG_VERSION");
    let repo = format_link(env!("CARGO_PKG_REPOSITORY"), env!("CARGO_PKG_REPOSITORY"));
    #[rustfmt::skip]
    let license = env!("CARGO_PKG_LICENSE")
        .replace("MIT", &format_link("MIT", "https://choosealicense.com/licenses/mit/"))
        .replace("Apache-2.0", &format_link("Apache-2.0", "https://choosealicense.com/licenses/apache-2.0/"));
    match locale!() {
        "zh-cn" => eprintlns!(
            "没有播放列表。";
            "用法: {program_name} <输入文件> [输入文件] ...";
            "{divider}";
            "tvid - 终端视频播放器";
            "版本: {version}";
            "仓库: {repo}";
            "许可: {license}";
        ),
        "zh-tw" => eprintlns!(
            "沒有播放清單。";
            "用法: {program_name} <輸入檔案> [輸入檔案] ...";
            "{divider}";
            "tvid - 終端機視訊播放器";
            "版本: {version}";
            "儲存庫: {repo}";
            "授權: {license}";
        ),
        "ja-jp" => eprintlns!(
            "プレイリストがありません。";
            "使用法: {program_name} <入力ファイル> [入力ファイル] ...";
            "{divider}";
            "tvid - ターミナルビデオプレーヤー";
            "バージョン: {version}";
            "リポジトリ: {repo}";
            "ライセンス: {license}";
        ),
        "fr-fr" => eprintlns!(
            "Aucune liste de lecture.";
            "Utilisation: {program_name} <fichier d'entrée> [fichier d'entrée] ...";
            "{divider}";
            "tvid - Lecteur vidéo terminal";
            "version: {version}";
            "dépôt: {repo}";
            "licence: {license}";
        ),
        "de-de" => eprintlns!(
            "Keine Wiedergabeliste.";
            "Verwendung: {program_name} <Eingabedatei> [Eingabedatei] ...";
            "{divider}";
            "tvid - Terminal Video Player";
            "Version: {version}";
            "Repository: {repo}";
            "Lizenz: {license}";
        ),
        "es-es" => eprintlns!(
            "No hay lista de reproducción.";
            "Uso: {program_name} <archivo de entrada> [archivo de entrada] ...";
            "{divider}";
            "tvid - Reproductor de video de terminal";
            "versión: {version}";
            "repositorio: {repo}";
            "licencia: {license}";
        ),
        _ => eprintlns!(
            "No input files as playlist.";
            "Usage: {program_name} <input> [input] ...";
            "{divider}";
            "tvid - Terminal Video Player";
            "version: {version}";
            "repo: {repo}";
            "license: {license}";
        ),
    }
}

fn print_help(program_name: &str) {
    let version = env!("CARGO_PKG_VERSION");
    let repo = format_link(env!("CARGO_PKG_REPOSITORY"), env!("CARGO_PKG_REPOSITORY"));
    #[rustfmt::skip]
    let license = env!("CARGO_PKG_LICENSE")
        .replace("MIT", &format_link("MIT", "https://choosealicense.com/licenses/mit/"))
        .replace("Apache-2.0", &format_link("Apache-2.0", "https://choosealicense.com/licenses/apache-2.0/"));
    match locale!() {
        "zh-cn" => eprintlns!(
            "tvid - 终端视频播放器";
            "版本: {version}";
            "仓库: {repo}";
            "许可: {license}";
            "";
            "用法: {program_name} <输入文件> [输入文件] ...";
            "";
            "控制:";
            "  空格          : 播放/暂停";
            "  q             : 退出";
            "  n             : 播放列表中下一个视频";
            "  l             : 切换播放列表显示";
            "  f             : 打开文件选择器";
            "  c             : 循环颜色模式";
        ),
        "zh-tw" => eprintlns!(
            "tvid - 終端機視訊播放器";
            "版本: {version}";
            "儲存庫: {repo}";
            "授權: {license}";
            "";
            "用法: {program_name} <輸入檔案> [輸入檔案] ...";
            "";
            "控制:";
            "  空白鍵        : 播放/暫停";
            "  q             : 離開";
            "  n             : 播放清單中的下一個視訊";
            "  l             : 切換播放清單顯示";
            "  f             : 開啟檔案選擇器";
            "  c             : 循環顏色模式";
        ),
        "ja-jp" => eprintlns!(
            "tvid - ターミナルビデオプレーヤー";
            "バージョン: {version}";
            "リポジトリ: {repo}";
            "ライセンス: {license}";
            "";
            "使用法: {program_name} <入力ファイル> [入力ファイル] ...";
            "";
            "コントロール:";
            "  スペースキー  : 再生/一時停止";
            "  q             : 終了";
            "  n             : プレイリストの次のビデオ";
            "  l             : プレイリスト表示の切り替え";
            "  f             : ファイルセレクターを開く";
            "  c             : カラーモードを切り替え";
        ),
        "fr-fr" => eprintlns!(
            "tvid - Lecteur vidéo terminal";
            "version: {version}";
            "dépôt: {repo}";
            "licence: {license}";
            "";
            "Utilisation: {program_name} <fichier d'entrée> [fichier d'entrée] ...";
            "";
            "Contrôles:";
            "  Espace        : Lecture/Pause";
            "  q             : Quitter";
            "  n             : Vidéo suivante dans la liste de lecture";
            "  l             : Basculer l'affichage de la liste de lecture";
            "  f             : Ouvrir le sélecteur de fichiers";
            "  c             : Changer le mode couleur";
        ),
        "de-de" => eprintlns!(
            "tvid - Terminal Video Player";
            "Version: {version}";
            "Repository: {repo}";
            "Lizenz: {license}";
            "";
            "Verwendung: {program_name} <Eingabedatei> [Eingabedatei] ...";
            "";
            "Steuerung:";
            "  Leertaste     : Abspielen/Pause";
            "  q             : Beenden";
            "  n             : Nächstes Video in der Wiedergabeliste";
            "  l             : Wiedergabelistenanzeige umschalten";
            "  f             : Dateiauswahl öffnen";
            "  c             : Farbmodus wechseln";
        ),
        "es-es" => eprintlns!(
            "tvid - Reproductor de video de terminal";
            "versión: {version}";
            "repositorio: {repo}";
            "licencia: {license}";
            "";
            "Uso: {program_name} <archivo de entrada> [archivo de entrada] ...";
            "";
            "Controles:";
            "  Espacio       : Reproducir/Pausar";
            "  q             : Salir";
            "  n             : Siguiente video en la lista de reproducción";
            "  l             : Alternar visualización de la lista de reproducción";
            "  f             : Abrir selector de archivos";
            "  c             : Cambiar modo de color";
        ),
        _ => eprintlns!(
            "tvid - Terminal Video Player";
            "version: {version}";
            "repo: {repo}";
            "license: {license}";
            "";
            "Usage: {program_name} <input> [input] ...";
            "";
            "Controls:";
            "  Space         : Play/Pause";
            "  q             : Quit";
            "  n             : Next video in playlist";
            "  l             : Toggle playlist display";
            "  f             : Open file selector";
            "  c             : Cycle color mode";
        ),
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "tvid",
    about = "Terminal Video Player",
    version = env!("CARGO_PKG_VERSION"),
    disable_help_flag = true,
    disable_help_subcommand = true,
)]
struct CliArgs {
    /// Print help and exit
    #[arg(short = 'h', long = "help")]
    show_help: bool,

    /// Input files (playlist)
    #[arg(value_name = "INPUT")]
    inputs: Vec<String>,

    /// Small seek step in seconds
    #[arg(long = "seek-small", default_value_t = 5.0)]
    seek_small: f64,

    /// Large seek step in seconds
    #[arg(long = "seek-large", default_value_t = 30.0)]
    seek_large: f64,

    #[arg(short = 'l', long = "loop")]
    loop_playlist: bool,

    #[arg(short = 'p', long = "playlist")]
    playlist: Option<String>,
}

static SEEK_SMALL_STEP: Mutex<f64> = Mutex::new(5.0);
static SEEK_LARGE_STEP: Mutex<f64> = Mutex::new(30.0);

fn register_input_callbacks() {
    stdin::register_keypress_callback(Key::Normal(' '), |_| {
        avsync::switch_pause_state();
        true
    });
    stdin::register_keypress_callback(Key::Normal('q'), |_| {
        QUIT_CONFIRMATION.store(true, Ordering::SeqCst);
        true
    });
    stdin::register_keypress_callback(Key::Normal('n'), |_| {
        ffmpeg::notify_quit();
        true
    });
    stdin::register_keypress_callback(Key::Normal('l'), |_| {
        playlist::toggle_show_playlist();
        true
    });
    stdin::register_keypress_callback(Key::Normal('m'), |_| true);
    stdin::register_keypress_callback(Key::Normal('f'), |_| {
        ui::FILE_SELECT.fetch_xor(true, Ordering::SeqCst);
        true
    });

    stdin::register_keypress_callback(Key::Normal('c'), |_| {
        COLOR_MODE.lock().switch_next();
        FORCEFLUSH_NEXT.store(true, Ordering::SeqCst);
        true
    });

    stdin::register_keypress_callback(Key::Up, |_| {
        seek_request_relative(-*SEEK_LARGE_STEP.lock());
        true
    });
    stdin::register_keypress_callback(Key::Down, |_| {
        seek_request_relative(*SEEK_LARGE_STEP.lock());
        true
    });
    stdin::register_keypress_callback(Key::Left, |_| {
        seek_request_relative(-*SEEK_SMALL_STEP.lock());
        true
    });
    stdin::register_keypress_callback(Key::Right, |_| {
        seek_request_relative(*SEEK_SMALL_STEP.lock());
        true
    });

    playlist::register_keypress_callbacks();
    ui::register_input_callbacks();
}

static APP_START_TIME: OnceLock<Instant> = OnceLock::new();

fn main() -> Result<()> {
    APP_START_TIME.set(Instant::now()).unwrap();

    let program_name = env::args().nth(0).unwrap_or_else(|| {
        eprintln_l10n!(
            "zh-cn" => "666 这几把的 0 个参数，没救了属于是";
            "zh-tw" => "媽的 0 個參數，什麼鳥東西";
            "ja-jp" => "なんだこれ、引数0個とかありえないだろ";
            "fr-fr" => "Qu'est-ce que c'est que ça, 0 arguments ?";
            "de-de" => "Was zum Teufel ist das, 0 Argumente?";
            "es-es" => "¿Qué diablos es esto, 0 argumentos?";
            _       => "Got 0 args? What the fuck?";
        );
        std::process::exit(1);
    });

    let cli = CliArgs::parse();
    *SEEK_SMALL_STEP.lock() = cli.seek_small;
    *SEEK_LARGE_STEP.lock() = cli.seek_large;

    #[cfg(feature = "config")]
    {
        config::create_if_not_exists(None)?;
        config::load(None)?;
    }

    if cli.show_help {
        print_help(&program_name);
        std::process::exit(0);
    }

    if !cli.inputs.is_empty() {
        PLAYLIST.lock().clear().extend(cli.inputs.clone());
    }

    // if let Some(playlist_path) = cli.playlist {
    //     PLAYLIST.lock().load_from_file(&playlist_path)?;
    // }

    if PLAYLIST.lock().len() == 0 {
        print_no_playlist(&program_name);
        std::process::exit(1);
    }

    av::init().context("av init failed")?;

    term::init();
    term::setup_panic_handler(); // 一定要在初始化之后设置，且必须立刻设置

    ffmpeg::init();

    register_input_callbacks();

    render::add_render_callback(render::render_video);
    #[cfg(feature = "subtitle")]
    render::add_render_callback(subtitle::render_subtitle);
    render::add_render_callback(ui::render_ui);

    let input_main = TOKIO_RUNTIME.spawn(stdin::input_main());
    let output_main = TOKIO_RUNTIME.spawn(stdout::output_main());
    let render_main = std::thread::spawn(render::render_main);

    let mut continuous_failure_count = 0;
    while let Some(path) = { PLAYLIST.lock().next().cloned() } {
        let success = ffmpeg::decode_main(&path).unwrap_or_else(|err| {
            error_l10n!(
                "zh-cn" => "ffmpeg 解码错误: {err}";
                "zh-tw" => "ffmpeg 解碼錯誤: {err}";
                "ja-jp" => "ffmpeg デコードエラー: {err}";
                "fr-fr" => "erreur de décodage ffmpeg: {err}";
                "de-de" => "ffmpeg Decode-Fehler: {err}";
                "es-es" => "error de decodificación de ffmpeg: {err}";
                _       => "ffmpeg decode error: {err}";
            );
            false
        });
        if success {
            continuous_failure_count = 0;
        } else {
            continuous_failure_count += 1;
        }
        if TERM_QUIT.load(Ordering::SeqCst) {
            break;
        }
        if continuous_failure_count >= PLAYLIST.lock().len() {
            error_l10n!(
                "zh-cn" => "连续失败次数过多，程序退出。";
                "zh-tw" => "連續失敗次數過多，程式退出。";
                "ja-jp" => "連続した失敗が多すぎます。終了します。";
                "fr-fr" => "Trop d'échecs consécutifs, sortie.";
                "de-de" => "Zu viele aufeinanderfolgende Fehler, Programm wird beendet.";
                "es-es" => "Demasiados fallos continuos, saliendo.";
                _       => "Too many continuous failures, exiting.";
            );
            break;
        }
    }

    term::request_quit();

    render_main.join().unwrap_or_else(|err| {
        error_l10n!(
            "zh-cn" => "渲染线程 join 错误: {err:?}";
            "zh-tw" => "渲染執行緒 join 錯誤: {err:?}";
            "ja-jp" => "レンダリングスレッドの join エラー: {err:?}";
            "fr-fr" => "Erreur de jointure du thread de rendu : {err:?}";
            "de-de" => "Render-Thread Join-Fehler: {err:?}";
            "es-es" => "Error al unir el hilo de render: {err:?}";
            _ => "render thread join error: {err:?}";
        );
    });
    TOKIO_RUNTIME.block_on(async {
        output_main.await.unwrap_or_else(|err| {
            error_l10n!(
                "zh-cn" => "输出任务 join 错误: {err:?}";
                "zh-tw" => "輸出任務 join 錯誤: {err:?}";
                "ja-jp" => "出力タスクの join エラー: {err:?}";
                "fr-fr" => "Erreur de jointure de la tâche de sortie : {err:?}";
                "de-de" => "Output-Task Join-Fehler: {err:?}";
                "es-es" => "Error de unión de la tarea de salida: {err:?}";
                _ => "output task join error: {err:?}";
            );
        });
        input_main.await.unwrap_or_else(|err| {
            error_l10n!(
                "zh-cn" => "输入任务 join 错误: {err:?}";
                "zh-tw" => "輸入任務 join 錯誤: {err:?}";
                "ja-jp" => "入力タスクの join エラー: {err:?}";
                "fr-fr" => "Erreur de jointure de la tâche d'entrée : {err:?}";
                "de-de" => "Input-Task Join-Fehler: {err:?}";
                "es-es" => "Error de unión de la tarea de entrada: {err:?}";
                _ => "input task join error: {err:?}";
            );
        });
    });

    #[cfg(feature = "config")]
    config::save(None).unwrap_or_else(|err| {
        error_l10n!(
            "zh-cn" => "配置保存错误: {err}";
            "zh-tw" => "配置儲存錯誤: {err}";
            "ja-jp" => "設定の保存エラー: {err}";
            "fr-fr" => "Erreur d'enregistrement de la configuration : {err}";
            "de-de" => "Fehler beim Speichern der Konfiguration: {err}";
            "es-es" => "Error al guardar la configuración: {err}";
            _ => "config save error: {err}";
        );
    });

    term::quit();
}
