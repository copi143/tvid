# <span style="font-variant:small-caps">Terminal VIDeo player</span>

<img align="left" width="192" src="../tvid.min.svg" alt="tvid logo" />

`tvid` — видеоплеер для терминала на Rust. Он использует FFmpeg для декодирования и рендерит видео, аудио и субтитры прямо в терминале, предоставляя оверлейный UI, список воспроизведения и базовые взаимодействия клавиатуры/мыши.

---

*Translations (by ChatGPT):*<br />
[en-us/English](../README.md) | [zh-cn/简体中文](README.zh-cn.md) | [zh-tw/繁體中文](README.zh-tw.md) | [ja-jp/日本語](README.ja-jp.md) | [fr-fr/Français](README.fr-fr.md) | [de-de/Deutsch](README.de-de.md) | [es-es/Español](README.es-es.md) | [ko-kr/한국어](README.ko-kr.md) | [pt-br/Português (Brasil)](README.pt-br.md) | **ru-ru/Русский** | [it-it/Italiano](README.it-it.md) | [tr-tr/Türkçe](README.tr-tr.md) | [vi-vn/Tiếng Việt](README.vi-vn.md)

<br clear="left"/>

---

> Проект активно развивается. Поведение и интерфейс могут изменяться.

## Возможности

- **Воспроизводит почти любой формат**, поддерживаемый FFmpeg
- **Вывод аудио и рендеринг субтитров** (ASS / текст)
- **Несколько режимов рендера**: true color, 256 цветов, градации серого, ASCII art, Braille Unicode
- **Опциональные протоколы изображений**: Sixel и OSC 1337 (стиль iTerm2)
- **Оверлейный UI в терминале**: прогресс-бар, сообщения и справка на экране
- **Поддержка плейлистов**:
  - передача нескольких файлов через командную строку
  - навигация по плейлисту в памяти (следующий/предыдущий, цикл)
  - опциональная боковая панель плейлиста
- **Управление мышью и клавиатурой** для перемотки и навигации
- **Файл конфигурации и плейлист по умолчанию** в `~/.config/tvid/`
- **Локализованный UI** (системная локаль) и **Unifont** для поддержки глифов

## Требования

- Актуальный toolchain Rust (**nightly не требуется**)
  - Debian / Ubuntu: `sudo apt install cargo` или `sudo apt install rustup && rustup install stable`
  - Arch: `sudo pacman -S rust` или `sudo pacman -S rustup && rustup install stable`
- Библиотеки FFmpeg и заголовки разработчика
  - Debian / Ubuntu: `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - Arch: `sudo pacman -S ffmpeg`

## Сборка и запуск

### Использование Cargo Install

Можно установить `tvid` напрямую через Cargo:

```sh
cargo install tvid
```

Опциональные фичи включаются при сборке. По умолчанию: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

```sh
cargo install tvid --features sixel,osc1337
# или отключить стандартные фичи и выбрать минимум
cargo install tvid --no-default-features --features ffmpeg,video
```

### Сборка вручную

1. Клонировать репозиторий:

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. Сборка проекта:

   ```sh
   cargo build --release
   ```

   Опциональные фичи включаются при сборке. По умолчанию: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

   ```sh
   cargo build --release --features sixel,osc1337
   # или отключить стандартные фичи и выбрать минимум
   cargo build --release --no-default-features --features ffmpeg,video
   ```

3. Запуск плеера:

   ```sh
   cargo run -- <input1> [input2] [...]
   # или после сборки
   target/release/tvid <input1> [input2] [...]
   ```

## Использование

```sh
tvid <input1> [input2] [...]
```

Каждый входной файл становится элементом плейлиста в памяти.

### Конфигурация и файлы плейлиста

При первом запуске `tvid` создаёт каталог и два файла:

- Каталог конфигурации: `~/.config/tvid/`
- Файл конфигурации: `tvid.toml`
  - пример ключей:
    - `volume` (`0`–`200`): начальная громкость
    - `looping` (`true` / `false`): зацикливание плейлиста
- Файл плейлиста: `playlist.txt`
  - каждая строка — путь к файлу
  - пустые строки и комментарии `#` игнорируются

При запуске `tvid` загружает `playlist.txt`, затем добавляет файлы из командной строки.

### Управление клавиатурой и мышью

Основные управление (глобально):

- `Space` – воспроизведение / пауза
- `q` – выход
- Стрелки – перемотка
  - `←` – назад на 5 секунд
  - `→` – вперёд на 5 секунд
  - `↑` – назад на 30 секунд
  - `↓` – вперёд на 30 секунд

Управление плейлистом:

- `n` – следующий элемент
- `l` – показать/скрыть боковую панель плейлиста
- В панели плейлиста:
  - `w` / `↑` – переместить выбор вверх
  - `s` / `↓` – вниз
  - `Space` / `Enter` – воспроизвести выбранный элемент
  - `q` – закрыть панель плейлиста

UI и другое:

- `f` – открыть выбор файла
- `c` – переключить цветовой режим
- Прогресс-бар:
  - левый клик рядом с нижней областью прогресса для перемотки
  - перетаскивание левой кнопкой для скраббинга

> Примечание: по мере развития проекта могут появляться новые сочетания и элементы UI.

### Командный режим

Нажмите `/`, чтобы открыть ввод команды, затем:

- `Enter` – выполнить команду
- `Esc` – отменить
- `Tab` – автодополнение (команды или аргументы)
- `↑` / `↓` – история команд

Примеры:

- `/seek +5`
- `/volume 80`
- `/lang zh-cn`

Доступные коды языка: `en-us`, `zh-cn`, `zh-tw`, `ja-jp`, `fr-fr`, `de-de`, `es-es`, `ko-kr`, `pt-br`, `ru-ru`, `it-it`, `tr-tr`, `vi-vn`.

## Устранение неполадок

- Ошибки сборки:
  - Убедитесь, что FFmpeg и его headers установлены.
- Ошибка загрузки shared libraries (при запуске):
  - Убедитесь, что компилируете и запускаете на одной машине.
  - Проверьте, доступны ли runtime-библиотеки FFmpeg (например, работает ли `vlc`).
- При старте: `av init failed`:
  - Проверьте, что FFmpeg работает корректно.
- После старта: `No input files.`:
  - Убедитесь, что:
    - передан хотя бы один доступный видео/аудио файл, или
    - `~/.config/tvid/playlist.txt` содержит валидные пути.

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
