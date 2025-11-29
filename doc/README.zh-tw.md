# tvid

`tvid` 是一個以 Rust 編寫的終端機影片播放器。它使用 FFmpeg 做解碼，並直接在終端中呈現影片、音訊與字幕，提供覆蓋式使用者介面、播放列表檢視，以及基本的鍵盤/滑鼠互動。

---

*翻譯：*<br />
[en-us/English](../README.md) | [zh-cn/简体中文](README.zh-cn.md) | **zh-tw/繁體中文**

*其他語言（由 ChatGPT 翻譯）：*<br />
[ja-jp/日本語](README.ja-jp.md) · [fr-fr/Français](README.fr-fr.md) · [de-de/Deutsch](README.de-de.md) · [es-es/Español](README.es-es.md)

---

> 本專案仍在積極開發中。行為與介面可能會變動。

## 功能

- **播放幾乎所有由 FFmpeg 支援的格式**
- **終端覆蓋式 UI**：進度列、訊息與畫面內說明
- **播放列表支援**：
  - 可在命令列一次傳入多個檔案
  - 記憶體內播放列表導覽（上一首 / 下一首、循環）
  - 可選的播放列表側邊欄
- **滑鼠與鍵盤控制**，用於跳轉與導航
- **設定檔與預設播放列表**位於 `~/.config/tvid/`
- 使用 **Unifont** 以在 UI 中獲得較佳字形覆蓋率

## 系統需求

- 近期的 Rust toolchain（不需要 nightly）
  - 在 Debian / Ubuntu：`sudo apt install cargo` 或 `sudo apt install rustup && rustup install stable`
  - 在 Arch：`sudo pacman -S rust` 或 `sudo pacman -S rustup && rustup install stable`
- 系統上需有 FFmpeg 函式庫與開發標頭
  - 在 Debian / Ubuntu：`sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - 在 Arch：`sudo pacman -S ffmpeg`

## 編譯與執行

1. 克隆倉庫：

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. 編譯專案：

   ```sh
   cargo build --release
   ```

3. 執行播放器：

   ```sh
   cargo run -- <輸入1> [輸入2] [...]
   # 或（編譯後）
   target/release/tvid <輸入1> [輸入2] [...]
   ```

命令列傳入的每個輸入都會成為記憶體中的播放列表項目。

## 用法

```sh
tvid <輸入1> [輸入2] [...]
```

### 設定檔與播放列表檔案

首次執行時，`tvid` 會建立一個設定目錄與兩個檔案：

- 設定目錄：`~/.config/tvid/`
- 設定檔：`tvid.toml`
  - 範例鍵：
    - `volume`（`0`–`200`）：初始音量
    - `looping`（`true` / `false`）：是否循環播放播放列表
- 播放列表檔：`playlist.txt`
  - 每行視為一個檔案路徑
  - 空行與以 `#` 開頭的註解行會被忽略

啟動時，`tvid` 會先從 `playlist.txt` 載入播放列表，然後將命令列傳入的檔案附加到列表後方。

### 鍵盤與滑鼠控制

主要播放控制（全域）：

- `Space` – 播放 / 暫停
- `q` – 退出播放器
- 方向鍵 – 跳轉
  - `←` – 後退 5 秒
  - `→` – 前進 5 秒
  - `↑` – 後退 30 秒
  - `↓` – 前進 30 秒

播放列表控制：

- `n` – 播放播放列表中的下一項
- `l` – 切換播放列表側邊欄顯示
- 在播放列表面板中：
  - `w` / `↑` – 選擇上移
  - `s` / `↓` – 選擇下移
  - `Space` / `Enter` – 播放選中的項目
  - `q` – 關閉播放列表面板

UI 與其他：

- `f` – 打開檔案選擇面板
- `c` – 切換配色模式
- 進度列：
  - 在底部進度區域左鍵點擊以跳轉
  - 按住左鍵拖曳可進行抓取（scrub）

> 注意：專案仍在快速迭代中，可能會新增或調整快捷鍵與 UI 元件。

## 疑難排解

- 編譯期間錯誤：
  - 請確認系統已安裝 FFmpeg 與其開發標頭。
- 執行時載入共享函式庫錯誤：
  - 請確認您在同一台機器上編譯並執行程式；其他機器可能有不同版本的 FFmpeg。
  - 確保能找到 FFmpeg 的執行時函式庫（例如，檢查 `vlc` 是否能正常運作）。
- 啟動時顯示 `av init failed`：
  - 檢查系統上的 FFmpeg 是否正常工作。
- 啟動後顯示 `No input files.`：
  - 請確認您已經：
    - 透過命令列傳入至少一個可讀取的影片/音訊檔案，或
    - `~/.config/tvid/playlist.txt` 包含有效且可存取的路徑。

## 授權

請參閱倉庫根目錄 `README.md` 中的 License（英文）段落，了解授權相關細節。
