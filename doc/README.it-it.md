# <span style="font-variant:small-caps">Terminal VIDeo player</span>

<img align="left" width="192" src="../tvid.min.svg" alt="tvid logo" />

`tvid` è un lettore video da terminale scritto in Rust. Usa FFmpeg per la decodifica e rende video, audio e sottotitoli direttamente nel terminale, con UI sovrapposta, vista playlist e interazioni base di mouse/tastiera.

---

*Translations (by ChatGPT):*<br />
[en-us/English](../README.md) | [zh-cn/简体中文](README.zh-cn.md) | [zh-tw/繁體中文](README.zh-tw.md) | [ja-jp/日本語](README.ja-jp.md) | [fr-fr/Français](README.fr-fr.md) | [de-de/Deutsch](README.de-de.md) | [es-es/Español](README.es-es.md) | [ko-kr/한국어](README.ko-kr.md) | [pt-br/Português (Brasil)](README.pt-br.md) | [ru-ru/Русский](README.ru-ru.md) | **it-it/Italiano** | [tr-tr/Türkçe](README.tr-tr.md) | [vi-vn/Tiếng Việt](README.vi-vn.md)

<br clear="left"/>

---

> Questo progetto è in sviluppo attivo. Comportamento e UI possono cambiare.

## Funzionalità

- **Riproduce quasi qualsiasi formato** supportato da FFmpeg
- **Uscita audio e rendering sottotitoli** (ASS / testo)
- **Più modalità di rendering**: true color, 256 colori, scala di grigi, ASCII art, braille Unicode
- **Protocolli immagine opzionali**: Sixel e OSC 1337 (stile iTerm2)
- **UI sovrapposta nel terminale**: barra di avanzamento, messaggi e aiuto su schermo
- **Supporto playlist**:
  - passare più file dalla riga di comando
  - navigazione playlist in memoria (successivo / precedente, loop)
  - pannello laterale playlist opzionale
- **Controllo mouse e tastiera** per seek e navigazione
- **File di configurazione e playlist predefinita** in `~/.config/tvid/`
- **UI localizzata** (locale di sistema) e fallback **Unifont** per i glifi

## Requisiti

- Toolchain Rust recente (**nightly non richiesto**)
  - Debian / Ubuntu: `sudo apt install cargo` oppure `sudo apt install rustup && rustup install stable`
  - Arch: `sudo pacman -S rust` oppure `sudo pacman -S rustup && rustup install stable`
- Librerie FFmpeg e header di sviluppo
  - Debian / Ubuntu: `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - Arch: `sudo pacman -S ffmpeg`

## Build e avvio

### Usare Cargo Install

Puoi installare `tvid` direttamente con Cargo:

```sh
cargo install tvid
```

Le feature opzionali si abilitano in fase di build. Predefinite: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

```sh
cargo install tvid --features sixel,osc1337
# oppure disabilita le predefinite e scegli il minimo
cargo install tvid --no-default-features --features ffmpeg,video
```

### Build manuale

1. Clona il repository:

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. Compila il progetto:

   ```sh
   cargo build --release
   ```

   Le feature opzionali si abilitano in fase di build. Predefinite: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

   ```sh
   cargo build --release --features sixel,osc1337
   # oppure disabilita le predefinite e scegli il minimo
   cargo build --release --no-default-features --features ffmpeg,video
   ```

3. Avvia il player:

   ```sh
   cargo run -- <input1> [input2] [...]
   # oppure, dopo la build
   target/release/tvid <input1> [input2] [...]
   ```

## Utilizzo

```sh
tvid <input1> [input2] [...]
```

Ogni input diventa un elemento della playlist in memoria.

### File di configurazione e playlist

Al primo avvio, `tvid` crea una directory di configurazione e due file:

- Directory di configurazione: `~/.config/tvid/`
- File di configurazione: `tvid.toml`
  - chiavi di esempio:
    - `volume` (`0`–`200`): volume iniziale
    - `looping` (`true` / `false`): ripetizione playlist
- File playlist: `playlist.txt`
  - ogni riga è trattata come percorso file
  - righe vuote e commenti `#` sono ignorati

All'avvio, `tvid` carica la playlist da `playlist.txt` e poi aggiunge i file passati dalla riga di comando.

### Controlli da tastiera e mouse

Controlli principali (globali):

- `Space` – riproduci / pausa
- `q` – esci
- Frecce – seek
  - `←` – indietro di 5 secondi
  - `→` – avanti di 5 secondi
  - `↑` – indietro di 30 secondi
  - `↓` – avanti di 30 secondi

Controlli playlist:

- `n` – prossimo elemento
- `l` – mostra/nascondi pannello playlist
- Nel pannello playlist:
  - `w` / `↑` – sposta selezione su
  - `s` / `↓` – sposta selezione giù
  - `Space` / `Enter` – riproduci elemento selezionato
  - `q` – chiudi pannello playlist

UI e altro:

- `f` – apri selettore file
- `c` – cambia modalità colore
- Barra di avanzamento:
  - click sinistro vicino all'area in basso per cercare
  - trascina con il mouse sinistro per scrub

> Nota: ulteriori scorciatoie e UI possono essere aggiunti in futuro.

### Modalità comando

Premi `/` per aprire l'input comandi, poi:

- `Enter` – esegui comando
- `Esc` – annulla
- `Tab` – autocompletamento (comandi o argomenti)
- `↑` / `↓` – cronologia comandi

Esempi:

- `/seek +5`
- `/volume 80`
- `/lang zh-cn`

Codici lingua disponibili: `en-us`, `zh-cn`, `zh-tw`, `ja-jp`, `fr-fr`, `de-de`, `es-es`, `ko-kr`, `pt-br`, `ru-ru`, `it-it`, `tr-tr`, `vi-vn`.

## Risoluzione dei problemi

- Errori di build:
  - Assicurati che FFmpeg e gli header di sviluppo siano installati.
- Errore di caricamento delle librerie condivise (runtime):
  - Compila ed esegui sullo stesso sistema.
  - Verifica che le librerie runtime di FFmpeg siano trovate (es. `vlc`).
- All'avvio: `av init failed`:
  - Verifica che FFmpeg funzioni correttamente.
- Dopo l'avvio: `No input files.`:
  - Assicurati che:
    - sia stato passato almeno un file leggibile, oppure
    - `~/.config/tvid/playlist.txt` contenga percorsi validi.

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
