<p align="center">
  <img src="assets/app-icon/filmcraft-1024.png" alt="FilmCraft's app icon" width="128">
</p>

<h1 align="center">FilmCraft for Android</h1>

<p align="center">
  <b>FilmCraft-android is an open-source reimplementation of the Adobe Premiere Android app, created using the same Rust engine, <a href="https://github.com/storytold/filmcraft">FilmCraft</a>, as the desktop app, and built with a Kotlin/Jetpack Compose UI framework.</b><br>
  Many credits to the original maintainers, <a href="https://github.com/storytold">storytold</a>.
</p>

<p align="center">
  <a href="https://github.com/artcraft-android/filmcraft-android/releases"><img src="https://raw.githubusercontent.com/Kunzisoft/Github-badge/main/get-it-on-github.png" alt="Get it on GitHub" height="60"></a>
  <a href="https://apps.obtainium.imranr.dev/redirect?r=obtainium://add/https://github.com/artcraft-android/filmcraft-android"><img src="https://raw.githubusercontent.com/ImranR98/Obtainium/main/assets/graphics/badge_obtainium.png" alt="Get it on Obtainium" height="60"></a>
</p>

<p align="center">
  <img alt="Rust engine" src="https://img.shields.io/badge/engine-Rust-lightblue?style=flat&logo=rust&logoColor=white">
  <img alt="Kotlin and Jetpack Compose" src="https://img.shields.io/badge/UI-Kotlin%20%C2%B7%20Jetpack%20Compose-blue?style=flat">
  <img alt="Minimum: Android 10 (API 29)" src="https://img.shields.io/badge/min-Android%2010%20%28API%2029%29-blue?style=flat">
  <img alt="Target: Android 16 (API 36)" src="https://img.shields.io/badge/target-Android%2016%20%28API%2036%29-blue?style=flat">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat">
  <img alt="Status: beta" src="https://img.shields.io/badge/status-beta-lightblue?style=flat">
</p>

<p align="center">
  <a href="#editing">Editing</a> ·
  <a href="#filters-and-transitions">Filters &amp; transitions</a> ·
  <a href="#text">Text</a> ·
  <a href="#captions-and-text-to-speech">Captions &amp; speech</a> ·
  <a href="#export-and-home">Export &amp; Home</a> ·
  <a href="#sideways">Sideways</a> ·
  <a href="#planned-features">Planned features</a> ·
  <a href="#building-instructions">Building</a>
</p>

<br>

<p align="center">
  <img src="docs/images/editor.png" alt="The editor: a colour-bars test clip under the playhead with the text Made with FilmCraft on an orange band, the transport row, the timeline with lanes for captions and text, three clips on the main track with their sound, and the tools Audio, Text, Captions, Files, Blank and Aspect ratio" width="250">
  <br>
  <sub>The editor: the preview, the transport row, a timeline with lanes for captions, text and sound, and the tools.</sub>
</p>

<br>

## Editing

<table align="center">
<tr>
<td align="center"><img src="docs/images/clip-tools.png" alt="A clip selected on the main track with its trim handles, and the clip tools: Split, Delete, Retime, Audio, Filters and Effects" width="250"></td>
<td align="center"><img src="docs/images/speed.png" alt="The Speed panel: a slider from 0.1x to 10x set at 1x, the clip's duration and a reset button" width="250"></td>
</tr>
<tr>
<td align="center"><sub>A clip selected: split, delete, retime, audio, filters, effects and more</sub></td>
<td align="center"><sub>Speed, from 0.1× to 10×</sub></td>
</tr>
</table>

<br>

## Filters and transitions

<table align="center">
<tr>
<td align="center"><img src="docs/images/filters.png" alt="The Filters panel: None, Colorful, Warm (chosen), Cool and Washed out, each a preview of the clip itself, and an Intensity slider" width="250"></td>
<td align="center"><img src="docs/images/transitions.png" alt="The Transitions panel at a cut: None, Dissolve, Push up, Push down and Push left (chosen), every tile animating, and a Duration slider at 0.4 s" width="250"></td>
</tr>
<tr>
<td align="center"><sub>31 filters, previewed on the clip</sub></td>
<td align="center"><sub>50 transitions, every tile animated</sub></td>
</tr>
</table>

<br>

## Masking

Select a video or image and open **Mask**. Add an ellipse, rectangle or polygon, or turn on **Draw**
and sketch one closed stroke on the preview. Drag a mask to move it, drag a corner to resize it, and
drag its points to reshape it; **Add point** inserts a point on the nearest edge. Feather, expansion,
mask opacity, invert and the combine modes shape the cut-out. Masks reveal the layers underneath,
follow the clip's placement, mirror and flip, and are saved with the project. Every drag is one undo
step, and the same Rust renderer draws masks in the preview and in exported video.

Masks live on the clip's Opacity effect in the Rust engine (`masks.*` commands); Kotlin only draws the
handles and sends source-pixel geometry back through the UniFFI bridge. Changing a filter or effect
keeps the clip's masks.

<br>

## Text

<p align="center">
  <img src="docs/images/text-style.png" alt="The Style panel for the text Made with FilmCraft: the Presets, Font, Color and Layout tabs and six preset looks, the orange box chosen" width="250">
  <br>
  <sub>Text: presets, the phone's fonts, colours and layout.</sub>
</p>

<br>

## Captions and text to speech

<table align="center">
<tr>
<td align="center"><img src="docs/images/captions.png" alt="Captions made on the phone: The quick brown fox jumps over the lazy dog. along the bottom of the frame, and the Captions panel with the spoken language, the speech model (Faster or More accurate), Make captions again, size, position and box" width="250"></td>
<td align="center"><img src="docs/images/speech.png" alt="The Text to speech panel: Welcome to my first FilmCraft edit., the voices Basic Female, Basic Male and Natural voices, Pace and Pitch sliders, Listen and Add" width="250"></td>
</tr>
<tr>
<td align="center"><sub>Captions, made on the phone</sub></td>
<td align="center"><sub>Text to speech, with built-in and natural voices</sub></td>
</tr>
</table>

<br>

## Export and Home

<table align="center">
<tr>
<td align="center"><img src="docs/images/export.png" alt="The Export screen: the first frame of the video, Export settings and Export video" width="250"></td>
<td align="center"><img src="docs/images/home.png" alt="Home: From your photo library, New from files, New blank project and Extract audio, then Your projects with one project dated Oct 14, 2026, and the add button" width="250"></td>
</tr>
<tr>
<td align="center"><sub>Export at 720p, 1080p or 4K, 24 to 60 fps</sub></td>
<td align="center"><sub>Home: four ways to start, then your projects</sub></td>
</tr>
</table>

<br>

## Sideways

<p align="center">
  <img src="docs/images/landscape.png" alt="The editor sideways: the transport on the left, the preview with the text Made with FilmCraft, and the timeline and tools on the right" width="500">
  <br>
  <sub>Sideways, the preview sits beside the timeline.</sub>
</p>

<br>

## Planned features

FilmCraft for Android is in beta.

| Area | Status |
|---|---|
| Projects from the photo library, files or blank; open shared videos; autosave; rename, duplicate, delete | ✅ |
| The timeline: trim, move, reorder, split, delete, duplicate, replace, undo and redo; pinch to zoom | ✅ |
| Speed (0.1–10×, the sound keeps its pitch), freeze frame, volume, fades, extract audio, voiceover | ✅ |
| 31 filters, 23 effects, opacity, mirror and flip; move, scale and turn on the preview | ✅ |
| Clip masks: ellipse, rectangle, polygon and free-draw; feather, expansion, opacity, invert and combine; drag on the preview | ✅ |
| 3D objects: primitives, extruded text and imported GLB/glTF models with PBR textures, rigs and animations; any size GLB imports | ✅ |
| 50 transitions, intro and outro, apply to all | ✅ |
| Text: presets, the phone's fonts, colours and layout | ✅ |
| Captions made on the phone (Whisper, downloaded once when you ask), edited and burned in | ✅ |
| Text to speech: two built-in voices and nine natural voices (Kokoro, downloaded once when you ask) | ✅ |
| 11 aspect ratios, blank clips | ✅ |
| Export at 720p, 1080p or 4K, 24 to 60 fps, saved to Movies and shared | ✅ |
| Upright and sideways layouts | ✅ |
| Stock music, templates | ⬜ |
| Generative AI, cloud storage, accounts | 🚫 FilmCraft is a local app |

<sub>✅ works today · 🚧 in progress · ⬜ not started · 🚫 not planned</sub>

<br>

## Building Instructions
*One Gradle command builds the whole app: it compiles the Rust engine with [cargo-ndk](https://github.com/bbqsrc/cargo-ndk) and generates the Kotlin bindings.*

* Clone this repository
* Make sure you have at least 4 GB of free disk space
* enter `apps/filmcraft-android/android` and run `./gradlew assembleRelease`, or open that folder in Android Studio and press Run.

Build time dependencies:

<details>
  <summary>required tools</summary>

  ```
      rustup (it installs the Rust release and Android targets rust-toolchain.toml names)
      cargo install cargo-ndk --version 4.1.2 --locked
      Android SDK (API 37) and NDK 29.0.14206865
      JDK 17 or newer
  ```
</details>

*Builds are only tested on Windows 11.*

<br>

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE), at your option. FilmCraft for Android is not affiliated with Adobe;
Premiere is a trademark of Adobe Inc.

<br>

## Misc info for myself

Try to use [semantic commit](https://kapeli.com/cheat_sheets/Semantic_Commits.docset/Contents/Resources/Documents/index)
messages. Use `feat:` / `fix:` / `chore:` / `docs:` / etc.
