<p align="center">
  <img src="docs/logo.png" width="110" alt="Logo de NoName Studio">
</p>

<h1 align="center">NoName Studio</h1>

<p align="center">
  Estudio de producción musical nativo para <b>Windows</b> y <b>Android</b>.<br>
  Haz beats, graba tu voz con autotune, edítala nota por nota, mezcla y masteriza. Sin suscripciones.
</p>

<p align="center">
  <img src="docs/capturas/playlist.png" alt="Playlist con el beat de ejemplo" width="900">
</p>

## Qué trae

**Componer**
- Channel Rack con secuenciador de pasos, Piano Roll con escalas e imán, arpegiador y swing.
- Instrumentos propios: batería, bajo, Rhodes, pad, campana, **NN Sampler** y el sintetizador **NN Wave** (12 presets).
- Plantillas para empezar rápido: Trap, Reggaetón, Boom bap, House, Lo-fi y Drill.
- MIDI: teclado en vivo, importar y exportar archivos `.mid`.
- Plugins **CLAP** como instrumentos y efectos (Windows).

**Grabar la voz**
- Escúchate mientras grabas, con nivelador automático, paneo y reverb.
- La toma queda tan fuerte y pareja como la escuchaste.
- Tomas por vuelta con bucle y punch, y comping para combinar las mejores partes.

**NN Tune (autotune)**
- 15 estilos en 4 categorías: Esenciales, Hip hop, Hyperpop y Ciencia ficción.
- Corrección precisa: con la voz real queda a 2–3 cents de la nota.
- Sigue los acordes de la canción, muestra la nota que cantas y dibuja tu voz en vivo.
- Se puede grabar con el autotune sin escucharse.

<p align="center">
  <img src="docs/capturas/nn-tune.png" alt="Panel del micrófono con NN Tune" width="900">
</p>

**Retune y armonías**
- Edita la voz **nota por nota**: arrastra notas, llévalas a la escala y quita el vaivén del tono.
- Compara original y con cambios mientras suena.
- Crea armonías (tercera, quinta u octava) en otra pista con un clic.

<p align="center">
  <img src="docs/capturas/retune.png" alt="Editor Retune" width="900">
</p>

**Mezclar y masterizar**
- Mixer con EQ, compresor, reverb, delay, NN Space y sidechain.
- Pistas con silencio, solo, volumen y paneo.
- Mastering con un clic: estilos **Universal, Fuego, Claridad y Cinta**, con intensidad y graves, medios y agudos.
- **Igualar a una canción de referencia**: copia su volumen y su balance de graves, medios y agudos.
- Medidor de sonoridad (LUFS) y efectos de voz listos (Voz limpia, Rap al frente, Trap brillante…).

<p align="center">
  <img src="docs/capturas/mastering.png" alt="Mastering con un clic" width="440">
  <img src="docs/capturas/mixer.png" alt="Mixer" width="440">
</p>

**Exportar**
- MP3, WAV (16 o 24 bits) y stems (una pista por canal), con normalización opcional.

**Más**
- Panel de letras y notas guardado en el proyecto.
- Tutorial interactivo la primera vez y copia de rescate si la app se cierra de golpe.
- Deshacer, versiones del proyecto y ajustes de audio, grabación y exportación.

## Descargar

Armado listo para usar en `dist/`:

| Plataforma | Archivo |
|---|---|
| Windows 10 / 11 | `NoNameStudio.exe` (no necesita instalación) |
| Android 8 o más nuevo | `NoNameStudio.apk` |

## Armar desde el código

Hace falta [Rust](https://rustup.rs) estable.

```powershell
# Windows: abrir la app
cargo run --release

# Windows + Android (necesita el SDK y NDK de Android y cargo-apk)
powershell -File tools\armar.ps1
```

## Pruebas

```powershell
cargo run --example features      # comprueba el motor: afinación, Retune, armonías, pistas, exportación…
cargo run --example rendimiento   # mide cuánto procesador usa cada proyecto
```

## Hecho con

[Rust](https://www.rust-lang.org) · [egui](https://github.com/emilk/egui) · [cpal](https://github.com/RustAudio/cpal) · [symphonia](https://github.com/pdeljanov/Symphonia) · [midir](https://github.com/Boddlnagg/midir) · [CLAP](https://cleveraudio.org)
