# tvid

`tvid` ist ein Terminal-Videoplayer, der in Rust geschrieben ist. Er verwendet FFmpeg zur Dekodierung und rendert Video, Audio und Untertitel direkt in Ihrem Terminal, mit einer Overlay-Oberfläche, Playlist-Ansicht und grundlegender Maus- / Tastatursteuerung.

---

*Übersetzungen:*<br />
[en-us/English](../README.md) | [zh-cn/简体中文](README.zh-cn.md)

*Weitere Sprachen (von ChatGPT übersetzt):*<br />
[ja-jp/日本語](doc/README.ja-jp.md) · [fr-fr/Français](doc/README.fr-fr.md) · **de-de/Deutsch** · [es-es/Español](doc/README.es-es.md)

---

> Dieses Projekt befindet sich in aktiver Entwicklung. Verhalten und UI können sich ändern.

## Funktionen

- **Wiedergabe fast aller Formate**, die von FFmpeg unterstützt werden
- **Terminal-Overlay-UI**: Fortschrittsleiste, Meldungen und Hilfe auf dem Bildschirm
- **Playlist-Unterstützung**:
  - mehrere Dateien über die Kommandozeile übergeben
  - Playlist-Navigation im Speicher (nächster / vorheriger Titel, Schleife)
  - optionales Playlist-Seitenpanel
- **Maus- & Tastatursteuerung** für Sprünge und Navigation
- **Konfigurationsdatei & Standard-Playlist** unter `~/.config/tvid/`
- Verwendet **Unifont** für eine bessere Glyphenabdeckung in der Overlay-UI

## Voraussetzungen

- Eine aktuelle Rust-Toolchain (nightly ist **nicht** erforderlich)
  - unter Debian / Ubuntu: `sudo apt install cargo` oder `sudo apt install rustup && rustup install stable`
  - unter Arch: `sudo pacman -S rust` oder `sudo pacman -S rustup && rustup install stable`
- FFmpeg-Bibliotheken und -Entwicklerheader müssen auf Ihrem System verfügbar sein
  - unter Debian / Ubuntu: `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - unter Arch: `sudo pacman -S ffmpeg`

## Build & Ausführung

1. Repository klonen:

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. Projekt bauen:

   ```sh
   cargo build --release
   ```

3. Player starten:

   ```sh
   cargo run -- <eingabe1> [eingabe2] [...]
   # oder, nach dem Bauen
   target/release/tvid <eingabe1> [eingabe2] [...]
   ```

Jede Eingabe wird zu einem Eintrag in der In-Memory-Playlist.

## Verwendung

```sh
tvid <eingabe1> [eingabe2] [...]
```

### Konfigurations- & Playlist-Dateien

Beim ersten Start erstellt `tvid` ein Konfigurationsverzeichnis und zwei Dateien:

- Konfigurationsverzeichnis: `~/.config/tvid/`
- Konfigurationsdatei: `tvid.toml`
  - Beispiel-Schlüssel:
    - `volume` (`0`–`200`): Anfangslautstärke
    - `looping` (`true` / `false`): ob die Playlist in Schleife abgespielt wird
- Playlist-Datei: `playlist.txt`
  - Zeilen werden als Dateipfade behandelt
  - leere Zeilen und Zeilen, die mit `#` beginnen, werden ignoriert

Beim Start lädt `tvid` zunächst die Playlist aus `playlist.txt` und fügt anschließend alle über die Kommandozeile übergebenen Dateien an.

### Tastatur- & Maussteuerung

Zentrale Wiedergabesteuerung (global):

- `Space` – Wiedergabe / Pause
- `q` – Player beenden
- Pfeiltasten – Springen
  - `←` – 5 Sekunden zurückspringen
  - `→` – 5 Sekunden vorspulen
  - `↑` – 30 Sekunden zurückspringen
  - `↓` – 30 Sekunden vorspulen

Playlist-Steuerung:

- `n` – nächstes Element in der Playlist abspielen
- `l` – Playlist-Seitenpanel ein-/ausblenden
- Im Playlist-Panel:
  - `w` / `↑` – Auswahl nach oben verschieben
  - `s` / `↓` – Auswahl nach unten verschieben
  - `Space` / `Enter` – ausgewähltes Element abspielen
  - `q` – Playlist-Panel schließen

UI & Sonstiges:

- `f` – Dateiauswahl öffnen (UI-Panel)
- `c` – Farbmodus durchschalten
- Fortschrittsleiste:
  - mit der linken Maustaste im unteren Fortschrittsbereich klicken, um zu springen
  - mit gedrückter linker Maustaste ziehen, um zu scrubben

> Hinweis: Zusätzliche Tastenkombinationen und UI-Elemente können im Laufe der Entwicklung hinzugefügt werden.

## Fehlerbehebung

- Build-Fehler während der Kompilierung:
  - Stellen Sie sicher, dass FFmpeg und die zugehörigen Entwicklerheader auf Ihrem System installiert sind.
- Fehler beim Laden von Shared Libraries (zur Laufzeit):
  - Stellen Sie sicher, dass Sie das Programm auf derselben Maschine kompiliert und ausgeführt haben — andere Maschinen können unterschiedliche FFmpeg-Versionen haben.
  - Vergewissern Sie sich, dass die FFmpeg-Laufzeitbibliotheken gefunden werden können (prüfen Sie z. B., ob andere FFmpeg-basierte Programme wie `vlc` korrekt laufen).
- Beim Start: `av init failed`:
  - Überprüfen Sie, ob FFmpeg auf Ihrem System korrekt funktioniert.
- Nach dem Start: `No input files.`:
  - Stellen Sie sicher, dass entweder
    - Sie mindestens eine lesbare Video-/Audiodatei über die Kommandozeile übergeben haben, oder
    - `~/.config/tvid/playlist.txt` gültige, zugreifbare Pfade enthält.

## Lizenz

Siehe den Abschnitt License in der `README.md`-Datei im Wurzelverzeichnis des Repositories (Englisch) für Details zur Lizenz.
