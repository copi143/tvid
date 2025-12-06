# tvid

`tvid` 是一个用 Rust 编写的终端视频播放器。它基于 FFmpeg 做解码，在你的终端中直接渲染视频、音频和字幕，并提供覆盖式 UI、播放列表视图以及基础的键盘 / 鼠标交互。

---

*翻译：*<br />
[en-us/English](README.md) | **zh-cn/简体中文**

*其他语言（由 ChatGPT 翻译）：*<br />
[ja-jp/日本語](doc/README.ja-jp.md) · [fr-fr/Français](doc/README.fr-fr.md) · [de-de/Deutsch](doc/README.de-de.md) · [es-es/Español](doc/README.es-es.md)

---

> 项目仍在积极开发中，行为和界面可能会有变动。

## 功能概览

- **几乎支持所有 FFmpeg 能解码的格式**
- **终端覆盖 UI**：底部进度条、消息提示、帮助面板等
- **播放列表支持**：
  - 命令行一次传入多个文件
  - 内存中的播放列表，支持上一首 / 下一首、循环
  - 可选播放列表侧边栏
- **键盘 + 鼠标控制**，包括快进 / 后退、切歌、拖动进度条等
- **配置文件和默认播放列表**：位于 `~/.config/tvid/`
- 使用 **Unifont** 作为 UI 字体源，以获得更好的字符覆盖率

## 环境依赖

- Rust 开发环境（**不需要** nightly）
  - Debian / Ubuntu: `sudo apt install cargo` 或 `sudo apt install rustup && rustup install stable`
  - Arch: `sudo pacman -S rust` 或 `sudo pacman -S rustup && rustup install stable`
- 系统中已安装 FFmpeg 运行库及开发头文件
  - Debian / Ubuntu: `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - Arch: `sudo pacman -S ffmpeg`

## 构建与运行

1. 克隆仓库：

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. 编译：

   ```sh
   cargo build --release
   ```

3. 运行播放器：

   ```sh
   cargo run -- <输入1> [输入2] [...]
   # 或直接使用已编译的二进制
   target/release/tvid <输入1> [输入2] [...]
   ```

## 基本用法

```sh
tvid <输入> [输入] ...
```

命令行传入的每个输入都会成为播放列表中的一项。

### 配置文件与播放列表

程序首次启动时会自动创建配置目录和两个文件：

- 配置目录：`~/.config/tvid/`
- 配置文件：`tvid.toml`
  - 示例配置项：
    - `volume`（`0`–`200`）：初始音量
    - `looping`（`true` / `false`）：是否循环播放播放列表
- 播放列表文件：`playlist.txt`
  - 每一行被视为一个文件路径
  - 空行和以 `#` 开头的行会被忽略

启动时，`tvid` 会先从 `playlist.txt` 读取播放列表，再将命令行传入的文件追加到列表后面。

### 键盘与鼠标操作

全局播放控制：

- `Space` – 播放 / 暂停
- `q` – 退出播放器
- 方向键 – 快进 / 后退
  - `←` – 后退 5 秒
  - `→` – 前进 5 秒
  - `↑` – 后退 30 秒
  - `↓` – 前进 30 秒

播放列表控制：

- `n` – 播放列表中的下一项
- `l` – 显示 / 隐藏播放列表侧边栏
- 在播放列表面板中：
  - `w` / `↑` – 光标上移
  - `s` / `↓` – 光标下移
  - `Space` / `Enter` – 播放当前选中项
  - `q` – 关闭播放列表面板

UI 与其他：

- `f` – 打开文件选择器面板
- `c` – 切换配色模式
- 进度条：
  - 在底部进度区域左键点击可跳转到对应位置
  - 按住左键拖动可以拖拽进度

> 项目仍在快速迭代中，后续可能会新增或调整快捷键与 UI 元素。

## 常见问题

- 编译时报错：
  - 请确认系统中已正确安装 FFmpeg **及其开发头文件**。
- 启动时报错 `error while loading shared libraries`：
  - 确保您是在本机上编译并运行程序，其它电脑上的 FFmpeg 版本可能不同。
  - 请确认 FFmpeg 运行库能被正确找到，至少如 vlc 等程序能正常运行。
- 启动时报错 `av init failed`：
  - 请检查 FFmpeg 能否正常使用。
- 启动后提示 `No input files.`：
  - 请确认：
    - 命令行至少传入了一个可用的视频 / 音频文件，或
    - `~/.config/tvid/playlist.txt` 中存在有效路径且文件可访问。

## 许可证

> 请参考仓库根目录下的 `README.md` 中的 License 部分，保持为英文原文。
