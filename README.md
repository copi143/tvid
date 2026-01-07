# <span style="font-variant:small-caps">Terminal VIDeo player</span>

`tvid` is a terminal video player written in Rust. It uses FFmpeg for decoding and renders video, audio and subtitles directly inside your terminal, providing an overlay UI, playlist view and basic mouse / keyboard interaction.

---

*Translations:*<br />
**en-us/English** | [zh-cn/简体中文](doc/README.zh-cn.md) | [zh-tw/繁體中文](doc/README.zh-tw.md)

*Other languages (translated by ChatGPT):*<br />
[ja-jp/日本語](doc/README.ja-jp.md) · [fr-fr/Français](doc/README.fr-fr.md) · [de-de/Deutsch](doc/README.de-de.md) · [es-es/Español](doc/README.es-es.md)

---

> This project is under active development. Behaviour and UI may change.

## Features

- **Play almost any format** supported by FFmpeg
- **Audio output and subtitle rendering** (ASS / text)
- **Multiple render modes**: true color, 256-color, grayscale, ASCII art, Unicode braille
- **Optional image protocols**: Sixel and OSC 1337 (iTerm2-style)
- **Terminal UI overlay**: progress bar, messages and on‑screen help
- **Playlist support**:
  - pass multiple files on the command line
  - in‑memory playlist navigation (next / previous, looping)
  - optional playlist side panel
- **Mouse & keyboard control** for seeking and navigation
- **Config file & default playlist** under `~/.config/tvid/`
- **Localized UI** (system locale) and **Unifont** fallback for glyph coverage

## Requirements

- A recent Rust toolchain (nightly is **not** required)
  - on Debian / Ubuntu: `sudo apt install cargo` or `sudo apt install rustup && rustup install stable`
  - on Arch: `sudo pacman -S rust` or `sudo pacman -S rustup && rustup install stable`
- FFmpeg libraries and development headers available on your system
  - on Debian / Ubuntu: `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - on Arch: `sudo pacman -S ffmpeg`

## Build & Run

### Using Cargo Install

You can install `tvid` directly using Cargo:

```sh
cargo install tvid
```

Optional features are enabled at build time. Defaults are `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

```sh
cargo install tvid --features sixel,osc1337
# or disable defaults and pick a minimal set
cargo install tvid --no-default-features --features ffmpeg,video
```

### Build It Manually

1. Clone the repository:

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. Build the project:

   ```sh
   cargo build --release
   ```

   With optional features:

   ```sh
   cargo build --release --features sixel,osc1337
   # or disable defaults and pick a minimal set
   cargo build --release --no-default-features --features ffmpeg,video
   ```

3. Run the player:

   ```sh
   cargo run -- <input1> [input2] [...]
   # or, after building
   target/release/tvid <input1> [input2] [...]
   ```

## Usage

```sh
tvid <input1> [input2] [...]
```

Each input becomes an item in the in‑memory playlist.

### Configuration & Playlist Files

On first run, `tvid` creates a config directory and two files:

- Config directory: `~/.config/tvid/`
- Config file: `tvid.toml`
  - example keys:
    - `volume` (`0`–`200`): initial volume
    - `looping` (`true` / `false`): whether to loop the playlist
- Playlist file: `playlist.txt`
  - lines are treated as file paths
  - blank lines and `#` comments are ignored

At startup, `tvid` loads the playlist from `playlist.txt` and then appends any files passed on the command line.

### Keyboard & Mouse Controls

Core playback controls (global):

- `Space` – play / pause
- `q` – quit player
- Arrow keys – seeking
  - `←` – seek backward 5 seconds
  - `→` – seek forward 5 seconds
  - `↑` – seek backward 30 seconds
  - `↓` – seek forward 30 seconds

Playlist controls:

- `n` – play next item in playlist
- `l` – toggle playlist side panel
- In playlist panel:
  - `w` / `↑` – move selection up
  - `s` / `↓` – move selection down
  - `Space` / `Enter` – play selected item
  - `q` – close playlist panel

UI & other controls:

- `f` – open file selector (UI panel)
- `c` – cycle color mode
- Progress bar:
  - left‑click near the bottom progress area to seek
  - drag with left mouse button to scrub

> Note: additional shortcuts and UI elements may be added while the project evolves.

## Troubleshooting

- Build errors during compilation:
  - Ensure FFmpeg and its development headers are installed on your system.
- Error while loading shared libraries (at runtime):
  - Make sure you compiled and run the program on the same machine — other machines may have different FFmpeg versions.
  - Ensure the FFmpeg runtime libraries can be found (for example, verify that other FFmpeg-using programs like `vlc` run correctly).
- At startup: `av init failed`:
  - Check that FFmpeg works correctly on your system.
- After startup: `No input files.`:
  - Ensure either:
    - you passed at least one readable video/audio file on the command line, or
    - `~/.config/tvid/playlist.txt` contains valid, accessible paths.

## License

Copyright (c) 2025 copi143

This project is dual-licensed under either:

- [MIT License (MIT)](LICENSE-MIT)
- [Apache License, Version 2.0 (Apache-2.0)](LICENSE-APACHE)

at your option.

You can choose either license according to your needs.

You may obtain copies of the licenses at:

- MIT: [LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>
- Apache-2.0: [LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>

Unless required by applicable law or agreed to in writing,
software distributed under these licenses is distributed on
an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
either express or implied. See the licenses for the specific
language governing permissions and limitations.

---

The file `unifont-17.0.01.bin` included in this project is a binary font file generated from the original `unifont-17.0.01.hex` source file using a Python script. The conversion process does not modify the font data, only its format.

The original Unifont files are dual-licensed under:

- GNU General Public License (GPL) version 2 or (at your option) any later version, with the GNU Font Embedding Exception
- SIL Open Font License (OFL) version 1.1

For details, see the included [OFL-1.1.txt](OFL-1.1.txt) file, or visit the [GNU Unifont website](https://unifoundry.com/unifont/index.html).

The Python script used for conversion is provided below for reference:

```python
with open('unifont-17.0.01.hex', 'r', encoding='utf-8') as f:
  lines = f.readlines()

result: list[bytearray | None] = [None] * 65536
for line in lines:
  if line.startswith('#') or line.strip() == '': continue
  parts = line.strip().split(':')
  if len(parts) != 2: continue
  codepoint = int(parts[0], 16)
  glyph_data = parts[1] if len(parts[1]) == 64 else ''.join([parts[1][i:i + 2] + '00' for i in range(0, 32, 2)])
  bytes_data = bytes(int(glyph_data[i:i + 2], 16) for i in range(0, 64, 2))
  data = bytearray(32)
  for y in range(0, 16):
    for x in range(0, 16):
      bit = (bytes_data[y * 2 + x // 8] >> (7 - x % 8)) & 1
      if bit:
        bidx = y // 4 * 8 + x // 2
        bbit = y % 4 * 2 + x % 2
        data[bidx] |= 1 << [0, 3, 1, 4, 2, 5, 6, 7][bbit]
  result[codepoint] = data

with open('unifont-17.0.01.bin', 'wb') as f:
  for entry in result:
    if entry is None:
      f.write(b'\x00' * 32)
    else:
      f.write(entry)
```
