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

One Rust test opens real audio devices (the default input and the first
"CABLE Input" output, or the default output when no cable exists), runs for a
few seconds and asserts there were no dropouts. It is skipped unless opted in:

```powershell
$env:WONKIXER_AUDIO_TESTS = "1"; cargo test -p wonkixer passes_audio
```

## Layout

- `src/`: TypeScript frontend. `main.ts` only bootstraps; each module owns one
  concern and takes its dependencies as arguments so tests can pass fakes.
- `crates/dsp/`: the signal chain as a pure library with no Tauri dependency.
  Most DSP iteration happens here with `cargo test -p wonkixer-dsp`.
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
