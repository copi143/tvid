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

This project is dual-licensed under either:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-MIT)

at your option.

You can choose either license according to your needs.
