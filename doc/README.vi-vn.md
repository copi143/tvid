# <span style="font-variant:small-caps">Terminal VIDeo player</span>

<img align="left" width="192" src="../tvid.min.svg" alt="tvid logo" />

`tvid` là trình phát video trong terminal viết bằng Rust. Nó dùng FFmpeg để giải mã và render video, âm thanh, phụ đề trực tiếp trong terminal, kèm UI phủ, danh sách phát và tương tác cơ bản bằng chuột/bàn phím.

---

*Translations (by ChatGPT):*<br />
[en-us/English](../README.md) | [zh-cn/简体中文](README.zh-cn.md) | [zh-tw/繁體中文](README.zh-tw.md) | [ja-jp/日本語](README.ja-jp.md) | [fr-fr/Français](README.fr-fr.md) | [de-de/Deutsch](README.de-de.md) | [es-es/Español](README.es-es.md) | [ko-kr/한국어](README.ko-kr.md) | [pt-br/Português (Brasil)](README.pt-br.md) | [ru-ru/Русский](README.ru-ru.md) | [it-it/Italiano](README.it-it.md) | [tr-tr/Türkçe](README.tr-tr.md) | **vi-vn/Tiếng Việt**

<br clear="left"/>

---

> Dự án đang được phát triển tích cực. Hành vi và giao diện có thể thay đổi.

## Tính năng

- **Phát gần như mọi định dạng** được FFmpeg hỗ trợ
- **Âm thanh và phụ đề** (ASS / văn bản)
- **Nhiều chế độ render**: true color, 256 màu, thang xám, ASCII art, Braille Unicode
- **Giao thức ảnh tùy chọn**: Sixel và OSC 1337 (kiểu iTerm2)
- **UI phủ trong terminal**: thanh tiến trình, thông báo và trợ giúp trên màn hình
- **Hỗ trợ danh sách phát**:
  - truyền nhiều tệp qua dòng lệnh
  - điều hướng danh sách phát trong bộ nhớ (tiếp theo / trước đó, lặp)
  - bảng bên danh sách phát tùy chọn
- **Điều khiển bằng chuột và bàn phím** cho seek và điều hướng
- **Tệp cấu hình & danh sách phát mặc định** tại `~/.config/tvid/`
- **UI đa ngôn ngữ** (theo locale hệ thống) và **Unifont** dự phòng glyph

## Yêu cầu

- Bộ công cụ Rust gần đây (**không cần** nightly)
  - Debian / Ubuntu: `sudo apt install cargo` hoặc `sudo apt install rustup && rustup install stable`
  - Arch: `sudo pacman -S rust` hoặc `sudo pacman -S rustup && rustup install stable`
- Thư viện FFmpeg và header phát triển
  - Debian / Ubuntu: `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - Arch: `sudo pacman -S ffmpeg`

## Build & Run

### Dùng Cargo Install

Bạn có thể cài `tvid` trực tiếp bằng Cargo:

```sh
cargo install tvid
```

Các tính năng tùy chọn bật khi build. Mặc định: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

```sh
cargo install tvid --features sixel,osc1337
# hoặc tắt mặc định và chọn tối thiểu
cargo install tvid --no-default-features --features ffmpeg,video
```

### Build thủ công

1. Clone repo:

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. Build:

   ```sh
   cargo build --release
   ```

   Các tính năng tùy chọn bật khi build. Mặc định: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

   ```sh
   cargo build --release --features sixel,osc1337
   # hoặc tắt mặc định và chọn tối thiểu
   cargo build --release --no-default-features --features ffmpeg,video
   ```

3. Chạy:

   ```sh
   cargo run -- <input1> [input2] [...]
   # hoặc sau khi build
   target/release/tvid <input1> [input2] [...]
   ```

## Sử dụng

```sh
tvid <input1> [input2] [...]
```

Mỗi đầu vào trở thành một mục trong danh sách phát trên bộ nhớ.

### Tệp cấu hình & danh sách phát

Lần chạy đầu, `tvid` tạo thư mục cấu hình và hai tệp:

- Thư mục cấu hình: `~/.config/tvid/`
- Tệp cấu hình: `tvid.toml`
  - khóa ví dụ:
    - `volume` (`0`–`200`): âm lượng ban đầu
    - `looping` (`true` / `false`): lặp danh sách phát
- Tệp danh sách phát: `playlist.txt`
  - mỗi dòng là một đường dẫn tệp
  - dòng trống và bình luận `#` bị bỏ qua

Khi khởi động, `tvid` đọc `playlist.txt` rồi thêm các tệp từ dòng lệnh.

### Điều khiển bàn phím & chuột

Điều khiển chính (toàn cục):

- `Space` – phát / tạm dừng
- `q` – thoát
- Phím mũi tên – seek
  - `←` – lùi 5 giây
  - `→` – tiến 5 giây
  - `↑` – lùi 30 giây
  - `↓` – tiến 30 giây

Điều khiển danh sách phát:

- `n` – mục tiếp theo
- `l` – bật/tắt bảng bên danh sách phát
- Trong bảng danh sách phát:
  - `w` / `↑` – lên
  - `s` / `↓` – xuống
  - `Space` / `Enter` – phát mục đã chọn
  - `q` – đóng bảng danh sách phát

UI và khác:

- `f` – mở bộ chọn tệp
- `c` – đổi chế độ màu
- Thanh tiến trình:
  - nhấp trái gần vùng tiến trình dưới để seek
  - kéo bằng chuột trái để scrub

> Lưu ý: có thể bổ sung thêm phím tắt và UI trong quá trình phát triển.

### Chế độ lệnh

Nhấn `/` để mở nhập lệnh, sau đó:

- `Enter` – chạy lệnh
- `Esc` – hủy
- `Tab` – tự động hoàn thành (lệnh hoặc tham số)
- `↑` / `↓` – lịch sử lệnh

Ví dụ:

- `/seek +5`
- `/volume 80`
- `/lang zh-cn`

Mã ngôn ngữ khả dụng: `en-us`, `zh-cn`, `zh-tw`, `ja-jp`, `fr-fr`, `de-de`, `es-es`, `ko-kr`, `pt-br`, `ru-ru`, `it-it`, `tr-tr`, `vi-vn`.

## Khắc phục sự cố

- Lỗi build:
  - Đảm bảo FFmpeg và các header phát triển đã được cài.
- Lỗi tải thư viện khi chạy (`error while loading shared libraries`):
  - Đảm bảo build và chạy trên cùng một máy.
  - Kiểm tra thư viện runtime FFmpeg có thể được tìm thấy (ví dụ `vlc`).
- Khi khởi động: `av init failed`:
  - Kiểm tra FFmpeg hoạt động bình thường.
- Sau khi khởi động: `No input files.`:
  - Đảm bảo:
    - bạn đã truyền ít nhất một tệp hợp lệ, hoặc
    - `~/.config/tvid/playlist.txt` có đường dẫn hợp lệ.

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
