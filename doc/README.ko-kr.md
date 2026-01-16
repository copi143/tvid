# <span style="font-variant:small-caps">Terminal VIDeo player</span>

<img align="left" width="192" src="../tvid.min.svg" alt="tvid logo" />

`tvid`는 Rust로 작성된 터미널 비디오 플레이어입니다. FFmpeg를 사용해 디코딩하고 터미널 안에서 영상/오디오/자막을 직접 렌더링하며, 오버레이 UI, 재생 목록 뷰, 기본적인 마우스/키보드 상호작용을 제공합니다.

---

*Translations (by ChatGPT):*<br />
[en-us/English](../README.md) | [zh-cn/简体中文](README.zh-cn.md) | [zh-tw/繁體中文](README.zh-tw.md) | [ja-jp/日本語](README.ja-jp.md) | [fr-fr/Français](README.fr-fr.md) | [de-de/Deutsch](README.de-de.md) | [es-es/Español](README.es-es.md) | **ko-kr/한국어** | [pt-br/Português (Brasil)](README.pt-br.md) | [ru-ru/Русский](README.ru-ru.md) | [it-it/Italiano](README.it-it.md) | [tr-tr/Türkçe](README.tr-tr.md) | [vi-vn/Tiếng Việt](README.vi-vn.md)

<br clear="left"/>

---

> 이 프로젝트는 활발히 개발 중입니다. 동작과 UI는 변경될 수 있습니다.

## 기능

- **FFmpeg가 지원하는 거의 모든 포맷 재생**
- **오디오 출력 및 자막 렌더링**(ASS / 텍스트)
- **다양한 렌더 모드**: True color, 256색, 그레이스케일, ASCII 아트, 유니코드 브라유
- **선택적 이미지 프로토콜**: Sixel, OSC 1337(iTerm2 스타일)
- **터미널 오버레이 UI**: 진행 바, 메시지, 화면 내 도움말
- **재생 목록 지원**:
  - 명령줄로 여러 파일 전달
  - 메모리 내 재생 목록 탐색(다음/이전, 반복)
  - 선택적 재생 목록 사이드 패널
- **시킹/탐색을 위한 마우스 & 키보드 컨트롤**
- **설정 파일 & 기본 재생 목록**: `~/.config/tvid/`
- **로컬라이즈된 UI**(시스템 로케일) 및 **Unifont** 글리프 보완

## 요구 사항

- 최신 Rust 툴체인( **nightly 불필요** )
  - Debian / Ubuntu: `sudo apt install cargo` 또는 `sudo apt install rustup && rustup install stable`
  - Arch: `sudo pacman -S rust` 또는 `sudo pacman -S rustup && rustup install stable`
- FFmpeg 라이브러리 및 개발 헤더
  - Debian / Ubuntu: `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - Arch: `sudo pacman -S ffmpeg`

## 빌드 & 실행

### Cargo Install 사용

Cargo로 `tvid`를 직접 설치할 수 있습니다:

```sh
cargo install tvid
```

옵션 기능은 빌드 시 활성화됩니다. 기본값은 `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont` 입니다.

```sh
cargo install tvid --features sixel,osc1337
# 또는 기본 기능을 끄고 최소 구성을 선택
cargo install tvid --no-default-features --features ffmpeg,video
```

### 수동 빌드

1. 저장소 클론:

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. 빌드:

   ```sh
   cargo build --release
   ```

   옵션 기능은 빌드 시 활성화됩니다. 기본값은 `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont` 입니다.

   ```sh
   cargo build --release --features sixel,osc1337
   # 또는 기본 기능을 끄고 최소 구성을 선택
   cargo build --release --no-default-features --features ffmpeg,video
   ```

3. 실행:

   ```sh
   cargo run -- <input1> [input2] [...]
   # 또는, 빌드 후
   target/release/tvid <input1> [input2] [...]
   ```

## 사용법

```sh
tvid <input1> [input2] [...]
```

각 입력은 메모리 내 재생 목록의 항목이 됩니다.

### 설정 & 재생 목록 파일

첫 실행 시 `tvid`는 설정 디렉터리와 두 개의 파일을 생성합니다:

- 설정 디렉터리: `~/.config/tvid/`
- 설정 파일: `tvid.toml`
  - 예시 키:
    - `volume` (`0`–`200`): 초기 볼륨
    - `looping` (`true` / `false`): 재생 목록 반복 여부
- 재생 목록 파일: `playlist.txt`
  - 각 줄은 파일 경로로 처리됨
  - 빈 줄과 `#` 주석은 무시됨

시작 시 `tvid`는 `playlist.txt`에서 재생 목록을 불러온 뒤, 명령줄 인자를 뒤에 추가합니다.

### 키보드 & 마우스 컨트롤

기본 재생 컨트롤(전역):

- `Space` – 재생 / 일시정지
- `q` – 종료
- 화살표 키 – 시킹
  - `←` – 5초 뒤로
  - `→` – 5초 앞으로
  - `↑` – 30초 뒤로
  - `↓` – 30초 앞으로

재생 목록 컨트롤:

- `n` – 다음 항목 재생
- `l` – 재생 목록 사이드 패널 토글
- 재생 목록 패널에서:
  - `w` / `↑` – 위로 이동
  - `s` / `↓` – 아래로 이동
  - `Space` / `Enter` – 선택 항목 재생
  - `q` – 재생 목록 패널 닫기

UI 및 기타:

- `f` – 파일 선택 패널 열기
- `c` – 색상 모드 전환
- 진행 바:
  - 하단 진행 영역에서 좌클릭하여 이동
  - 좌클릭 드래그로 스크럽

> 참고: 프로젝트가 발전하면서 단축키와 UI 요소가 추가/변경될 수 있습니다.

### 명령 모드

`/`를 눌러 명령 입력을 열고 다음을 사용할 수 있습니다:

- `Enter` – 명령 실행
- `Esc` – 취소
- `Tab` – 자동 완성(명령 또는 인자)
- `↑` / `↓` – 명령 기록

예시:

- `/seek +5`
- `/volume 80`
- `/lang zh-cn`

사용 가능한 언어 코드: `en-us`, `zh-cn`, `zh-tw`, `ja-jp`, `fr-fr`, `de-de`, `es-es`, `ko-kr`, `pt-br`, `ru-ru`, `it-it`, `tr-tr`, `vi-vn`.

## 문제 해결

- 컴파일 중 빌드 오류:
  - FFmpeg와 개발 헤더가 설치되어 있는지 확인하세요.
- 실행 중 `error while loading shared libraries` 오류:
  - 동일한 머신에서 빌드/실행했는지 확인하세요. 다른 머신은 FFmpeg 버전이 다를 수 있습니다.
  - FFmpeg 런타임 라이브러리를 찾을 수 있는지 확인하세요(예: `vlc` 등).
- 시작 시 `av init failed`:
  - FFmpeg가 정상 동작하는지 확인하세요.
- 시작 후 `No input files.`:
  - 다음 중 하나를 확인하세요:
    - 명령줄에 읽을 수 있는 파일을 하나 이상 전달함
    - `~/.config/tvid/playlist.txt`에 유효한 경로가 있음

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
