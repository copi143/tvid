# tvid

`tvid` es un reproductor de vídeo para terminal escrito en Rust. Utiliza FFmpeg para la decodificación y muestra vídeo, audio y subtítulos directamente en tu terminal, con una interfaz superpuesta, vista de lista de reproducción e interacción básica con ratón y teclado.

---

*Traducciones:*<br />
[en-us/English](../README.md) | [zh-cn/简体中文](README.zh-cn.md)

*Otros idiomas (traducidos por ChatGPT):*<br />
[ja-jp/日本語](doc/README.ja-jp.md) · [fr-fr/Français](doc/README.fr-fr.md) · [de-de/Deutsch](doc/README.de-de.md) · **es-es/Español**

---

> Este proyecto está en desarrollo activo. El comportamiento y la interfaz pueden cambiar.

## Características

- **Reproduce casi cualquier formato** compatible con FFmpeg
- **Salida de audio y renderizado de subtítulos** (ASS / texto)
- **Varios modos de renderizado**: color verdadero, 256 colores, escala de grises, arte ASCII, braille Unicode
- **Protocolos de imagen opcionales**: Sixel y OSC 1337 (estilo iTerm2)
- **Interfaz superpuesta en el terminal**: barra de progreso, mensajes y ayuda en pantalla
- **Soporte de lista de reproducción**:
  - pasar varios archivos por la línea de comandos
  - navegación en la lista de reproducción en memoria (anterior / siguiente, bucle)
  - panel lateral de lista de reproducción opcional
- **Control con ratón y teclado** para buscar y navegar
- **Archivo de configuración y lista de reproducción por defecto** en `~/.config/tvid/`
- **Interfaz localizada** (según la configuración regional del sistema) y **Unifont** como respaldo de glifos

## Requisitos

- Una toolchain reciente de Rust (nightly **no** es necesaria)
  - en Debian / Ubuntu: `sudo apt install cargo` o `sudo apt install rustup && rustup install stable`
  - en Arch: `sudo pacman -S rust` o `sudo pacman -S rustup && rustup install stable`
- Bibliotecas y cabeceras de desarrollo de FFmpeg disponibles en tu sistema
  - en Debian / Ubuntu: `sudo apt install ffmpeg libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev`
  - en Arch: `sudo pacman -S ffmpeg`

## Compilar y ejecutar

1. Clona el repositorio:

   ```sh
   git clone https://github.com/copi143/tvid.git
   cd tvid
   ```

2. Compila el proyecto:

   ```sh
   cargo build --release
   ```

   Las funciones opcionales se habilitan al compilar. Por defecto: `ffmpeg`, `i18n`, `config`, `audio`, `video`, `subtitle`, `unicode`, `unifont`.

   ```sh
   cargo build --release --features sixel,osc1337
   # o desactivar las funciones por defecto y elegir un mínimo
   cargo build --release --no-default-features --features ffmpeg,video
   ```

3. Ejecuta el reproductor:

   ```sh
   cargo run -- <entrada1> [entrada2] [...]
   # o, después de compilar
   target/release/tvid <entrada1> [entrada2] [...]
   ```

## Uso

```sh
tvid <entrada1> [entrada2] [...]
```

Cada entrada se convierte en un elemento de la lista de reproducción en memoria.

### Archivos de configuración y lista de reproducción

En la primera ejecución, `tvid` crea un directorio de configuración y dos archivos:

- Directorio de configuración: `~/.config/tvid/`
- Archivo de configuración: `tvid.toml`
  - claves de ejemplo:
    - `volume` (`0`–`200`): volumen inicial
    - `looping` (`true` / `false`): si repetir la lista de reproducción
- Archivo de lista de reproducción: `playlist.txt`
  - cada línea se trata como una ruta de archivo
  - se ignoran las líneas en blanco y las que comienzan por `#`

Al inicio, `tvid` carga la lista de reproducción desde `playlist.txt` y luego añade cualquier archivo pasado por línea de comandos.

### Controles de teclado y ratón

Controles básicos de reproducción (globales):

- `Space` – reproducir / pausar
- `q` – salir del reproductor
- Flechas – búsqueda
  - `←` – retroceder 5 segundos
  - `→` – avanzar 5 segundos
  - `↑` – retroceder 30 segundos
  - `↓` – avanzar 30 segundos

Controles de la lista de reproducción:

- `n` – reproducir el siguiente elemento de la lista
- `l` – mostrar / ocultar el panel lateral de la lista
- En el panel de lista de reproducción:
  - `w` / `↑` – mover la selección hacia arriba
  - `s` / `↓` – mover la selección hacia abajo
  - `Space` / `Enter` – reproducir el elemento seleccionado
  - `q` – cerrar el panel de lista de reproducción

Interfaz y otros:

- `f` – abrir el selector de archivos (panel de UI)
- `c` – cambiar el modo de color
- Barra de progreso:
  - clic izquierdo cerca del área inferior de progreso para buscar
  - arrastrar con el botón izquierdo del ratón para desplazarse

> Nota: se pueden añadir más atajos y elementos de interfaz a medida que el proyecto evolucione.

## Solución de problemas

- Errores de compilación:
  - Asegúrate de que FFmpeg y sus cabeceras de desarrollo estén instalados en tu sistema.
- Error al cargar bibliotecas compartidas (en tiempo de ejecución):
  - Asegúrate de compilar y ejecutar el programa en la misma máquina — otras máquinas pueden tener versiones diferentes de FFmpeg.
  - Verifica que las bibliotecas en tiempo de ejecución de FFmpeg se puedan encontrar (por ejemplo, comprueba que otros programas que usan FFmpeg como `vlc` funcionen correctamente).
- Al inicio: `av init failed`:
  - Comprueba que FFmpeg funcione correctamente en tu sistema.
- Después del inicio: `No input files.`:
  - Asegúrate de que:
    - has pasado al menos un archivo de vídeo/audio legible por línea de comandos, o
    - `~/.config/tvid/playlist.txt` contiene rutas válidas y accesibles.

## Licencia

Consulta la sección License del `README.md` en la raíz del repositorio (en inglés) para ver los detalles de la licencia.
