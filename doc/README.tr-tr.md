# <span style="font-variant:small-caps">Terminal VIDeo player</span>

<img align="left" width="192" src="../tvid.min.svg" alt="tvid logo" />

`tvid`, Rust ile yazılmış bir terminal video oynatıcısıdır. FFmpeg ile çözümleme yapar ve video, ses ile altyazıları doğrudan terminalde render eder; kaplamalı UI, çalma listesi görünümü ve temel klavye/fare etkileşimleri sunar.

---

*Translations (by ChatGPT):*<br />
[en-us/English](../README.md) | [zh-cn/简体中文](README.zh-cn.md) | [zh-tw/繁體中文](README.zh-tw.md) | [ja-jp/日本語](README.ja-jp.md) | [fr-fr/Français](README.fr-fr.md) | [de-de/Deutsch](README.de-de.md) | [es-es/Español](README.es-es.md) | [ko-kr/한국어](README.ko-kr.md) | [pt-br/Português (Brasil)](README.pt-br.md) | [ru-ru/Русский](README.ru-ru.md) | [it-it/Italiano](README.it-it.md) | **tr-tr/Türkçe** | [vi-vn/Tiếng Việt](README.vi-vn.md)

<br clear="left"/>

---

> Bu proje aktif geliştirme sürecindedir. Davranış ve arayüz değişebilir.

## Özellikler

- **FFmpeg'in desteklediği neredeyse tüm formatları oynatır**
- **Ses çıkışı ve altyazı render** (ASS / metin)
- **Çoklu render modları**: true color, 256 renk, gri tonlama, ASCII art, Unicode Braille
- **İsteğe bağlı görüntü protokolleri**: Sixel ve OSC 1337 (iTerm2 tarzı)
- **Terminal kaplama arayüz**: ilerleme çubuğu, mesajlar ve ekran içi yardım
- **Çalma listesi desteği**:
  - komut satırından birden fazla dosya
  - bellek içi çalma listesi gezintisi (sonraki/önceki, döngü)
  - isteğe bağlı yan panel
- **Arama ve gezinme için klavye & fare kontrolü**
- **Yapılandırma dosyası ve varsayılan çalma listesi** `~/.config/tvid/` altında
- **Yerelleştirilmiş UI** (sistem yereli) ve **Unifont** yedek glif desteği

## Gereksinimler

- Güncel Rust araç zinciri (**nightly gerekmez**)
  - Debian / Ubuntu: `sudo apt install cargo` veya `sudo apt install rustup && rustup install stable`
  - Arch: `sudo pacman -S rust` veya `sudo pacman -S rustup && rustup install stable`
- FFmpeg kütüphaneleri ve geliştirme başlıkları
  - Debian / Ubuntu: `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - Arch: `sudo pacman -S ffmpeg`

## Derleme ve çalıştırma

### Cargo Install kullan

`Cargo` ile `tvid` doğrudan kurulabilir:

```sh
cargo install tvid
```

Opsiyonel özellikler derleme sırasında etkinleştirilir. Varsayılanlar: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

```sh
cargo install tvid --features sixel,osc1337
# veya varsayılanları kapatıp minimum seçin
cargo install tvid --no-default-features --features ffmpeg,video
```

### Manuel derleme

1. Depoyu klonlayın:

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. Derleyin:

   ```sh
   cargo build --release
   ```

   Opsiyonel özellikler derleme sırasında etkinleştirilir. Varsayılanlar: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

   ```sh
   cargo build --release --features sixel,osc1337
   # veya varsayılanları kapatıp minimum seçin
   cargo build --release --no-default-features --features ffmpeg,video
   ```

3. Çalıştırın:

   ```sh
   cargo run -- <input1> [input2] [...]
   # veya derleme sonrası
   target/release/tvid <input1> [input2] [...]
   ```

## Kullanım

```sh
tvid <input1> [input2] [...]
```

Her giriş bellek içi çalma listesine eklenir.

### Yapılandırma ve çalma listesi dosyaları

İlk çalıştırmada `tvid` bir yapılandırma dizini ve iki dosya oluşturur:

- Yapılandırma dizini: `~/.config/tvid/`
- Yapılandırma dosyası: `tvid.toml`
  - örnek anahtarlar:
    - `volume` (`0`–`200`): başlangıç ses düzeyi
    - `looping` (`true` / `false`): çalma listesini döngüle
- Çalma listesi dosyası: `playlist.txt`
  - her satır bir dosya yoludur
  - boş satırlar ve `#` yorumları yok sayılır

Başlangıçta `tvid`, `playlist.txt`'ten listeyi yükler ve komut satırıyla gelen dosyaları ekler.

### Klavye ve fare kontrolleri

Temel kontroller (global):

- `Space` – oynat / duraklat
- `q` – çık
- Ok tuşları – arama
  - `←` – 5 saniye geri
  - `→` – 5 saniye ileri
  - `↑` – 30 saniye geri
  - `↓` – 30 saniye ileri

Çalma listesi kontrolleri:

- `n` – sonraki öğe
- `l` – çalma listesi panelini aç/kapat
- Çalma listesi panelinde:
  - `w` / `↑` – seçimi yukarı taşı
  - `s` / `↓` – seçimi aşağı taşı
  - `Space` / `Enter` – seçili öğeyi oynat
  - `q` – paneli kapat

UI ve diğer:

- `f` – dosya seçiciyi aç
- `c` – renk modunu değiştir
- İlerleme çubuğu:
  - alttaki ilerleme alanına sol tıklayıp arama
  - sol tuş basılıyken sürükleyerek scrub

> Not: Proje geliştikçe yeni kısayollar ve UI öğeleri eklenebilir.

### Komut modu

`/` ile komut girişini açın, sonra:

- `Enter` – komutu çalıştır
- `Esc` – iptal
- `Tab` – otomatik tamamlama (komut/argüman)
- `↑` / `↓` – komut geçmişi

Örnekler:

- `/seek +5`
- `/volume 80`
- `/lang zh-cn`

Kullanılabilir dil kodları: `en-us`, `zh-cn`, `zh-tw`, `ja-jp`, `fr-fr`, `de-de`, `es-es`, `ko-kr`, `pt-br`, `ru-ru`, `it-it`, `tr-tr`, `vi-vn`.

## Sorun giderme

- Derleme hataları:
  - FFmpeg ve geliştirme başlıklarının kurulu olduğundan emin olun.
- Çalıştırmada `error while loading shared libraries` hatası:
  - Derleme ve çalıştırmanın aynı makinede yapıldığını doğrulayın.
  - FFmpeg çalışma zamanı kütüphanelerinin bulunabildiğini kontrol edin (`vlc` gibi).
- Başlangıçta `av init failed`:
  - FFmpeg'in doğru çalıştığını kontrol edin.
- Başlangıç sonrası `No input files.`:
  - Şunlardan emin olun:
    - en az bir okunabilir dosya verilmiş olması, veya
    - `~/.config/tvid/playlist.txt` içinde geçerli yollar bulunması.

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
