# Development

## Prerequisites

- Windows 10 or Windows 11, x64
- Node.js 22 or later, with npm
- A current stable Rust toolchain (MSVC target)
- The [Tauri 2 Windows prerequisites](https://v2.tauri.app/start/prerequisites/)
- VB-Audio Virtual Cable, so a "CABLE Input" output device exists to route to

## Setup

```powershell
npm install
npm run tauri dev
```

The app keeps running in the tray when its window is closed: quit from the
tray menu, or stop `tauri dev` with Ctrl+C. Only one copy runs at a time, so
launching it again (an installed build next to a dev build included) just
brings the running window forward. Turning on launch at login from a dev
build registers the debug executable; turn it off again before relying on an
installed build.

## Checks

`npm run check` runs everything below in sequence.

```powershell
npm test
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Frontend tests run with Node's test runner, `tsx` and a JSDOM built from the
real `index.html`, importing TypeScript modules directly. Rust tests live next
to the code they cover.

One Rust test opens real audio devices: the default input, or the first
other input when the default is the cable's own "CABLE Output", and the first
"CABLE Input" output, or the default output when no cable exists. It runs for
at least 10 s, prints the measured latency every second, and asserts there
were no dropouts and that the worst latency after the first second stayed
under 45 ms on the low-latency output (80 ms on the fallback). From the third
to the sixth second it also auditions a quiet gated 330 Hz tone to the default
output (or the first output that is not the cable), so expect a soft beeping
in the headphones; the route must stay free of dropouts while it plays. It is
skipped unless opted in; `WONKIXER_SOAK_SECONDS` lengthens the run:

```powershell
$env:WONKIXER_AUDIO_TESTS = "1"; cargo test -p wonkixer passes_audio -- --nocapture
$env:WONKIXER_SOAK_SECONDS = "60"; cargo test -p wonkixer passes_audio -- --nocapture
```

### Latency

The footer's latency is measured from the Windows audio clock (see
`latencyMs` in the [UI contract](UI-CONTRACT.md)). Where it comes from, with
the HyperX headset into VB-Cable:

- The microphone delivers 10 ms packets and cannot go shorter, so the
  newest sample in hand can be up to 10 ms old.
- The ring between the two devices holds one input packet, two output
  periods and a 2 ms margin, about 17-18 ms counted from capture. It steers
  on the age of its oldest frame rather than on how full it is, so it stays
  safe whatever the phase between the two devices' clocks
  (`engine/transport.rs`).
- The output (`engine/wasapi.rs`) runs in 2.7 ms periods with a 280-frame
  buffer; the cable reports consuming a block 5-13 ms after it is handed
  over, depending on its phase when the route starts.
- The limiter's lookahead adds 5 ms.

A device without a low-latency shared mode, or one that does not mix in
32-bit float, goes through cpal instead (`engine/output.rs`): 10 ms periods
and a 1056-frame buffer, about 70 ms in total.

Device recovery is checked by hand: start the engine, unplug the headset (or
its wireless dongle), confirm the error names the lost device and the state
says it is waiting, plug it back in and confirm the engine is live again within
a few seconds. Restarting the Windows Audio service (`Restart-Service
Audiosrv` in an elevated PowerShell) exercises the same path. The resampler
runs whenever the input and output rates differ; to hear it, set the
microphone to 44 100 Hz under Sound settings, device properties, Advanced.

## Working on the sound

Render a recording through the chain without running the app:

```powershell
cargo run -p wonkixer-dsp --release --example offline -- input.wav output.wav
cargo run -p wonkixer-dsp --release --example offline -- input.wav output.wav "Broadcast"
```

The input can be any mono or stereo WAV; the output is mono 32-bit float at the
same rate and the command prints the peak gain reduction of each dynamics
stage. Record the raw microphone with Audacity (or any recorder) to get a
sample to iterate on, and keep such recordings under the git-ignored
`docs/samples/`.

Print the parameter schema the frontend is generated from:

```powershell
cargo run -p wonkixer-dsp --example dump_schema
```

## Layout

- `src/`: TypeScript frontend. `main.ts` only bootstraps; each module owns one
  concern and takes its dependencies as arguments so tests can pass fakes.
- `crates/dsp/`: the signal chain as a pure library with no Tauri dependency.
  Most DSP iteration happens here with `cargo test -p wonkixer-dsp`. The
  parameter table in `params.rs` is the single source of truth for stages,
  parameters, ranges and defaults; everything else (presets, the chain, the
  frontend schema) derives from it.
- `src-tauri/`: the Tauri shell, audio engine thread and commands. The engine
  thread owns the audio streams; commands reach it over a channel and it
  publishes `engine:state` and `engine:meters` events back to the window.
- `tests/`: frontend tests, discovered by glob.

## Conventions

- Semantic versioning. The version lives in `package.json` and the workspace
  `Cargo.toml`; `tauri.conf.json` inherits it.
- Conventional commit messages.
- UI changes are reviewed before they are committed.
- No code comments unless the code cannot be made readable any other way.
