# <span style="font-variant:small-caps">Terminal VIDeo player</span>

<img align="left" width="192" src="../tvid.min.svg" alt="tvid logo" />

`tvid` é um reprodutor de vídeo em terminal escrito em Rust. Ele usa o FFmpeg para decodificação e renderiza vídeo, áudio e legendas diretamente no terminal, oferecendo UI sobreposta, visualização de playlist e interação básica por teclado/mouse.

---

*Translations (by ChatGPT):*<br />
[en-us/English](../README.md) | [zh-cn/简体中文](README.zh-cn.md) | [zh-tw/繁體中文](README.zh-tw.md) | [ja-jp/日本語](README.ja-jp.md) | [fr-fr/Français](README.fr-fr.md) | [de-de/Deutsch](README.de-de.md) | [es-es/Español](README.es-es.md) | [ko-kr/한국어](README.ko-kr.md) | **pt-br/Português (Brasil)** | [ru-ru/Русский](README.ru-ru.md) | [it-it/Italiano](README.it-it.md) | [tr-tr/Türkçe](README.tr-tr.md) | [vi-vn/Tiếng Việt](README.vi-vn.md)

<br clear="left"/>

---

> Este projeto está em desenvolvimento ativo. O comportamento e a interface podem mudar.

## Funcionalidades

- **Reproduz quase qualquer formato** suportado pelo FFmpeg
- **Saída de áudio e renderização de legendas** (ASS / texto)
- **Vários modos de renderização**: true color, 256 cores, escala de cinza, ASCII art, braille Unicode
- **Protocolos de imagem opcionais**: Sixel e OSC 1337 (estilo iTerm2)
- **UI sobreposta no terminal**: barra de progresso, mensagens e ajuda na tela
- **Suporte a playlist**:
  - passar vários arquivos na linha de comando
  - navegação de playlist em memória (próximo / anterior, loop)
  - painel lateral opcional de playlist
- **Controle por teclado e mouse** para busca e navegação
- **Arquivo de configuração e playlist padrão** em `~/.config/tvid/`
- **UI localizada** (locale do sistema) e fallback **Unifont** para glifos

## Requisitos

- Toolchain Rust recente (**nightly não é necessário**)
  - Debian / Ubuntu: `sudo apt install cargo` ou `sudo apt install rustup && rustup install stable`
  - Arch: `sudo pacman -S rust` ou `sudo pacman -S rustup && rustup install stable`
- Bibliotecas do FFmpeg e headers de desenvolvimento
  - Debian / Ubuntu: `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - Arch: `sudo pacman -S ffmpeg`

## Compilar e executar

### Usando Cargo Install

Você pode instalar `tvid` diretamente com Cargo:

```sh
cargo install tvid
```

Recursos opcionais são habilitados na compilação. Padrão: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

```sh
cargo install tvid --features sixel,osc1337
# ou desative os padrões e escolha o mínimo
cargo install tvid --no-default-features --features ffmpeg,video
```

### Compilar manualmente

1. Clone o repositório:

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. Compile:

   ```sh
   cargo build --release
   ```

   Recursos opcionais são habilitados na compilação. Padrão: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

   ```sh
   cargo build --release --features sixel,osc1337
   # ou desative os padrões e escolha o mínimo
   cargo build --release --no-default-features --features ffmpeg,video
   ```

3. Execute:

   ```sh
   cargo run -- <input1> [input2] [...]
   # ou, após compilar
   target/release/tvid <input1> [input2] [...]
   ```

## Uso

```sh
tvid <input1> [input2] [...]
```

Cada entrada vira um item da playlist em memória.

### Arquivos de configuração e playlist

Na primeira execução, o `tvid` cria o diretório de configuração e dois arquivos:

- Diretório de configuração: `~/.config/tvid/`
- Arquivo de configuração: `tvid.toml`
  - chaves de exemplo:
    - `volume` (`0`–`200`): volume inicial
    - `looping` (`true` / `false`): repetir playlist
- Arquivo de playlist: `playlist.txt`
  - cada linha é tratada como caminho de arquivo
  - linhas em branco e comentários com `#` são ignorados

Na inicialização, o `tvid` carrega a playlist de `playlist.txt` e depois adiciona os arquivos passados pela linha de comando.

### Controles de teclado e mouse

Controles principais (globais):

- `Space` – reproduzir / pausar
- `q` – sair
- Setas – buscar
  - `←` – voltar 5 segundos
  - `→` – avançar 5 segundos
  - `↑` – voltar 30 segundos
  - `↓` – avançar 30 segundos

Controles da playlist:

- `n` – próximo item
- `l` – alternar painel da playlist
- No painel de playlist:
  - `w` / `↑` – mover seleção para cima
  - `s` / `↓` – mover seleção para baixo
  - `Space` / `Enter` – reproduzir item selecionado
  - `q` – fechar painel da playlist

UI e outros:

- `f` – abrir seletor de arquivos
- `c` – alternar modo de cor
- Barra de progresso:
  - clique esquerdo perto da barra inferior para buscar
  - arrastar com botão esquerdo para scrub

> Observação: atalhos e elementos de UI podem ser adicionados com o tempo.

### Modo de comandos

Pressione `/` para abrir a entrada de comandos, então:

- `Enter` – executar comando
- `Esc` – cancelar
- `Tab` – autocompletar (comandos ou argumentos)
- `↑` / `↓` – histórico de comandos

Exemplos:

- `/seek +5`
- `/volume 80`
- `/lang zh-cn`

Códigos de idioma disponíveis: `en-us`, `zh-cn`, `zh-tw`, `ja-jp`, `fr-fr`, `de-de`, `es-es`, `ko-kr`, `pt-br`, `ru-ru`, `it-it`, `tr-tr`, `vi-vn`.

## Solução de problemas

- Erros de compilação:
  - Verifique se o FFmpeg e seus headers de desenvolvimento estão instalados.
- Erro ao carregar bibliotecas compartilhadas (em execução):
  - Compile e execute no mesmo computador — outras máquinas podem ter versões diferentes do FFmpeg.
  - Verifique se as bibliotecas de runtime do FFmpeg podem ser encontradas (por exemplo, se `vlc` funciona).
- Ao iniciar: `av init failed`:
  - Verifique se o FFmpeg funciona corretamente.
- Após iniciar: `No input files.`:
  - Certifique-se de que:
    - você passou pelo menos um arquivo de vídeo/áudio legível, ou
    - `~/.config/tvid/playlist.txt` contém caminhos válidos.

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
