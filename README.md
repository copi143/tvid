# tvid

**This project is under active development. Features and functionality may change.**

`tvid` is a command-line video player written in Rust, utilizing FFmpeg for media decoding and playback. It supports video, audio, and subtitle streams, and provides a terminal-based user interface for interactive control.

## Features (TODO)

- [x] Play various video and audio formats via FFmpeg
- [ ] Terminal-based UI for playback control
- [x] Subtitle support
- [ ] Playlist management
- [ ] Audio and video stream selection
- [ ] Keyboard shortcuts for common actions

## Requirements

- Rust (latest stable version recommended)
- FFmpeg libraries installed on your system

## Build & Run

1. Clone the repository:

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. Build the project:

   ```sh
   cargo build --release
   ```

3. Run the player:

   ```sh
   cargo run -- <video-file>
   ```

   Or use the compiled binary in `target/release/tvid`.

## Usage

```sh
tvid <video-file> [video-file] ...
```

### Options (TODO)

- [ ] `--playlist <file>`: Load a playlist file
- [ ] `--subtitle <file>`: Load external subtitle

### Keyboard Shortcuts (TODO)

- [x] `Space`: Play/Pause
- [x] `q`: Quit
- [ ] `←/→`: Seek backward/forward
- [ ] `↑/↓`: Volume up/down
- [ ] `s`: Toggle subtitles

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
