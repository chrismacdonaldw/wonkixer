# wonkixer

wonkixer runs a broadcast-style voice chain on your microphone in real time
and hands the result to Discord, OBS, Zoom or any other app as a virtual
microphone. It aims for no perceptible latency and no artifacts first, then
tone.

[Download the latest release](https://github.com/chrismacdonaldw/wonkixer/releases/latest)

## Requirements

- Windows 11, x64
- [VB-Audio Virtual Cable](https://vb-audio.com/Cable/), which provides the
  virtual microphone

## Setting up VB-Cable

1. Download VB-Cable, extract the zip, run `VBCABLE_Setup_x64.exe` as
   administrator, choose **Install Driver** and restart Windows.
2. In **Settings > System > Sound**, open **CABLE Input** under Output and your
   microphone under Input, and set the **Format** of both to 48,000 Hz.
3. In wonkixer, choose your microphone as the input and **CABLE Input** as the
   output. In Discord, OBS, Zoom or any other app, pick **CABLE Output** as the
   microphone.

Keep your speakers or headphones as the Windows default output device;
otherwise system sounds end up in the cable.

### Discord

While wonkixer is your microphone, open **User Settings > Voice & Video** in
Discord and turn off **Noise Suppression**, **Echo Cancellation** and
**Automatic Gain Control**. They process the already-processed voice a second
time and fight the chain, which sounds like pumping and muffling.

## Installation

1. Open the [latest release](https://github.com/chrismacdonaldw/wonkixer/releases/latest).
2. Download the installer ending in `_x64-setup.exe` and run it. It installs
   for your Windows user only and needs no administrator rights.
3. Launch wonkixer and pick your devices as described above.

The installer is not Authenticode-signed, so Windows SmartScreen may show an
unrecognized-app warning; choose **More info > Run anyway**. To update, run a
newer installer over the existing installation. Settings and presets are kept.

## Building from source

With Node.js 22+, a stable Rust MSVC toolchain and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) installed:

```powershell
npm ci
npm run tauri dev
npm run check
```

`npm run check` runs the frontend tests, the production build, `cargo fmt`,
`cargo clippy` and the Rust tests. See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)
for details and [docs/RELEASING.md](docs/RELEASING.md) for producing a release.

## How it works

```
mic ─► wonkixer ─► CABLE Input ═ VB-Cable ═ CABLE Output ─► Discord / OBS / Zoom
```

wonkixer reads the microphone, processes it as mono at 48 kHz and plays the
result into CABLE Input; VB-Cable presents that as the CABLE Output microphone.
The chain runs in a fixed order:

input trim ─► high-pass ─► noise suppression ─► expander ─► de-esser ─► EQ ─►
compressor ─► limiter ─► output trim

- **High-pass** removes rumble and handling noise below the voice.
- **Noise suppression** is an optional neural denoiser, off by default, that
  adds 10 ms of latency while it is on.
- **Expander** quietens background noise in the gaps between phrases.
- **De-esser** tames harsh "s" and "t" sounds.
- **EQ** shapes the tone.
- **Compressor** evens out loud and quiet speech.
- **Limiter** keeps peaks under a ceiling so the output never clips.

Every stage between the trims can be bypassed, and every parameter changes
live without clicks. Five built-in presets cover common setups, and your own
presets are saved alongside them. The architecture and roadmap are in
[PLAN.md](PLAN.md); the parameters, presets and engine surface are in
[docs/UI-CONTRACT.md](docs/UI-CONTRACT.md), and calibration from recorded
takes is described in [docs/CALIBRATE.md](docs/CALIBRATE.md).
