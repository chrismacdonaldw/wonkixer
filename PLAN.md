# mixer — project plan

_Working title "mixer". Written 2026-10-09. A Windows desktop app that takes a
microphone, runs the standard broadcast voice chain on it in real time, and
exposes the result as a virtual microphone to Discord, OBS, Zoom and friends.
Priorities, in order: no perceptible latency, no artifacts, then tone._

---

## 1. Goals and non-goals

**Goals**

- Total added latency under ~40 ms end to end. The user's current NVIDIA
  Broadcast path adds ~500 ms and audible fuzz; both must be gone.
- The six-stage chain: high-pass, downward expander, de-esser, EQ, compressor,
  limiter. Every stage bypassable, every parameter live.
- Zero audio artifacts from the app itself: no clicks on slider moves, no
  dropouts, no latency creep over a 3-hour session.
- Set-and-forget: restores devices and settings on launch, starts processing
  automatically, lives in the tray.
- Looks and feels like `wonkitch` (spacing, type, density, chrome) with its own
  palette.

**Non-goals (for v1)**

- Noise *removal* under speech (neural denoise). The expander handles noise
  between phrases; the headset's close-mic position handles the rest. RNNoise
  is an optional stage in M7, off by default.
- Multi-source mixing, game/music ducking, VST hosting, macOS/Linux.
- Writing a virtual audio driver. We use VB-Audio Virtual Cable.

---

## 2. Environment facts (measured 2026-10-09)

Probe run with cpal 0.18 on this machine (WASAPI host):

| Device | Direction | Format |
|---|---|---|
| Microphone (HyperX Cloud Alpha Wireless) | input | F32, 1 ch, 48 000 Hz |
| Microphone (NVIDIA Broadcast) | input | F32, 2 ch, 48 000 Hz |
| Headphones (HyperX Cloud Alpha Wireless) | output | F32, 2 ch, 48 000 Hz |
| All other outputs | output | F32, 2 ch, 48 000 Hz |

- Everything is already 48 kHz, so no resampler is required on day one. The
  engine must still *detect* a mismatch and refuse with a clear message rather
  than produce chipmunk audio (M1), and resampling via `rubato` is a later
  upgrade (M6).
- VB-Cable is **not installed yet**. Installing it is step one of M0. After
  install, "CABLE Input" appears as an output device and "CABLE Output" as an
  input device.
- Toolchain: Rust 1.94 stable MSVC, Node 24, npm 11. No C++ toolchain, none
  needed.

---

## 3. Architecture

```
                  ┌────────────────────────────────────────────────────┐
                  │ Tauri main thread (WebView2 + command handlers)    │
                  │   commands: list_devices, start_engine, set_param… │
                  │   events out: engine:state, engine:meters          │
                  └────────────┬───────────────────────┬───────────────┘
      std::sync::mpsc commands │          Arc<Params>  │ (atomics, lock-free)
                               ▼                       │
                  ┌────────────────────────┐           │
                  │ Engine thread          │           │
                  │  owns cpal Streams     │           │
                  │  33 ms tick → meters   │           │
                  │  stream error handling │           │
                  └──────┬──────────┬──────┘           │
          builds streams │          │                  │
                         ▼          ▼                  ▼
  mic ──► [input callback] ──► SPSC ring ──► [output callback: DSP chain] ──► CABLE Input
           (WASAPI thread)                    (WASAPI thread, real-time)         │
                                                                                 ▼
                                                        Discord / OBS select "CABLE Output" as mic
```

**Threads and rules**

1. **Tauri main thread.** Owns the webview. Commands are thin: they send an
   `EngineCommand` over a channel or write a parameter atomic. Never blocks on
   audio.
2. **Engine thread** (`std::thread`, spawned in Tauri `setup`). Owns both cpal
   `Stream`s. This is mandatory, not stylistic: cpal's `Stream` is `!Send`, so
   it cannot live in Tauri managed state. The thread loops on
   `recv_timeout(33ms)`: handle commands, then read meter atomics and
   `app.emit("engine:meters", …)`. It also receives stream error callbacks
   (device unplugged) and emits `engine:state`.
3. **Input callback** (WASAPI thread). Converts to mono (average channels),
   pushes into a lock-free SPSC ring (`ringbuf`). Records input peak into an
   atomic. Nothing else.
4. **Output callback** (WASAPI thread, the one with the deadline). Pops one
   block from the ring, runs the chain, fans mono out to all output channels,
   records output peak and per-stage gain reduction. On ring underrun writes
   silence and bumps an `xruns` counter.

**Audio-thread discipline** (both callbacks): no allocation, no locks, no
logging, no Tauri calls, no `Instant::now()` in hot loops, no panics. Enforce in
debug builds with the `assert_no_alloc` crate wrapping the callback bodies.
Set FTZ/DAZ on the output thread at stream start so denormals in filter tails
don't spike CPU.

**Parameter path.** `Params` is a struct of `AtomicU32` (f32 bit patterns) plus
one `AtomicU32` generation counter. UI → Tauri command → store + bump
generation. The chain checks the generation once per block; if changed, it
reloads targets. Gains and thresholds are smoothed per-sample (one-pole, ~10 ms)
so sliders never zipper. Filter coefficients are recomputed per block from the
smoothed cutoff/gain/Q (trig per block is cheap at 100 blocks/s).

**Meter path.** Peaks are non-negative f32, whose bit patterns order the same
as `u32`, so `AtomicU32::fetch_max` gives lock-free per-block peak hold. The
engine thread swaps them to 0 each tick after reading. Gain reduction per stage
is stored the same way (as positive dB of reduction).

**Clock drift.** The mic and VB-Cable run on independent clocks. Even at a
nominal 48 kHz they drift a few ppm, so the ring either slowly fills (latency
creeps up) or starves (periodic dropouts). This is the thing vibe-coded audio
apps get wrong. M1 handles it: target fill = one block; if fill exceeds
target + 1 block, drop a sample per block until back in band; if it is starving,
duplicate one. Sub-audible at ppm rates. M6 can replace this with `rubato`
asynchronous resampling driven by the fill level if it ever proves audible.

**Latency budget (expected on this machine)**

| Component | ms |
|---|---|
| WASAPI shared-mode input period | 10 |
| Ring buffer target fill | ~10 |
| WASAPI shared-mode output period | 10 |
| Limiter lookahead | 5 |
| VB-Cable internal buffer (tunable in its control panel) | 5–20 |
| **Total** | **~40–55 before tuning, ~30–40 after** |

cpal on WASAPI uses shared mode only; 10 ms is the floor per side without
IAudioClient3 low-latency periods. The output side now talks to WASAPI
directly via the `windows` crate with `IAudioClient3::InitializeSharedAudioStream`
at the 2.67 ms period; the microphone allows nothing below 10 ms. Measured
numbers are in section 12, Latency.

---

## 4. Signal chain specification

Order is fixed in v1 (the screenshot's order, which is the textbook order):

```
in ─► gain ─► HPF ─► expander ─► de-esser ─► EQ ─► compressor ─► limiter ─► out gain ─► hard clip ─► out
```

All processing is mono f32 at the device rate. Every stage has `bypass: bool`
and `process(&mut self, x: f32) -> f32` (or per-block where needed). Dynamics
stages expose `gain_reduction_db()` for metering.

### 4.1 Building blocks (crate `dsp`, no Tauri dependency)

- **Biquad** — RBJ Audio EQ Cookbook, transposed direct form II. Types: HPF,
  LPF, peaking, low shelf, high shelf, band-pass. Coefficient recompute is a
  function of (type, f0, Q/slope, gain dB, sample rate).
- **Smoother** — one-pole toward a target, time constant in ms. Used for every
  user-facing continuous parameter.
- **Level detector** — peak detector in the log domain with separate attack and
  release coefficients (branching). Returns level in dBFS.
- **Gain computer** — static curve with soft knee, per Giannoulis, Massberg &
  Reiss, *Digital Dynamic Range Compressor Design — A Tutorial and Analysis*
  (JAES 2012). Parameterised by threshold, ratio, knee width. Used by
  compressor (ratio > 1 above threshold), expander (ratio applied *below*
  threshold with a `range` floor), de-esser (band-limited sidechain) and
  limiter (ratio ∞, knee 0).
- **Delay line** — fixed-length ring for limiter lookahead.
- **dB helpers** — `db_to_lin`, `lin_to_db` with a floor at -120 dB.

### 4.2 Stages, parameters and defaults

Defaults are tuned for a close-talk headset mic. Ranges are what the UI exposes.

| # | Stage | Parameter | Default | Range | Notes |
|---|---|---|---|---|---|
| 0 | Input gain | gain | 0 dB | -24…+24 dB | Pre-chain trim. Aim for peaks around -12 dBFS raw. |
| 1 | High-pass | cutoff | 90 Hz | 40…300 Hz | 2nd-order Butterworth (Q 0.707). Optional 4th-order toggle (two cascaded) in M5. |
| 2 | Downward expander | threshold | -45 dBFS | -80…-20 | Below this, signal is turned down. |
|   |  | ratio | 1:2.5 | 1:1…1:8 | Gentle. A gate is ratio ∞; we never want that. |
|   |  | range | -30 dB | -60…0 | Maximum attenuation. Never fully mutes, so no "dead air" switching. |
|   |  | attack | 2 ms | 0.1…50 | Fast so word onsets aren't clipped. |
|   |  | hold | 80 ms | 0…500 | Holds open across short gaps between words. |
|   |  | release | 150 ms | 10…1000 | Fades noise out instead of chopping. |
|   |  | knee | 6 dB | 0…12 | Soft knee hides the transition. |
| 3 | De-esser | frequency | 6 500 Hz | 3 000…10 000 | Centre of sibilance band. |
|   |  | threshold | -30 dBFS | -60…0 | Detector sees the high band only. |
|   |  | ratio | 4:1 | 1…10 | |
|   |  | attack / release | 0.5 / 40 ms | fixed-ish; expose in "advanced" | Must be fast; an "s" is ~50–100 ms. |
|   |  | mode | split-band | split-band / wideband | Split: `y = x - (1 - g) · HP(x)`, i.e. only the high band is ducked, so voice body is untouched. Wideband ducks everything (classic, slightly dulls). |
| 4 | EQ | low shelf | 120 Hz, 0 dB | ±12 dB | Body / boom. |
|   |  | low-mid peak | 250 Hz, -2.5 dB, Q 1.0 | ±12 dB, Q 0.3…4 | Mud cut. Biggest single improvement on headset mics. |
|   |  | presence peak | 4 000 Hz, +2.5 dB, Q 0.8 | ±12 dB, Q 0.3…4 | Clarity / "radio". |
|   |  | high shelf | 10 000 Hz, +1 dB | ±12 dB | Air. Keep small; the de-esser is before this. |
| 5 | Compressor | threshold | -18 dBFS | -60…0 | |
|   |  | ratio | 3:1 | 1…20 | |
|   |  | knee | 6 dB | 0…12 | |
|   |  | attack | 8 ms | 0.1…100 | Lets consonants through. |
|   |  | release | 120 ms | 10…1000 | |
|   |  | makeup | +6 dB | 0…+24 | Auto-makeup toggle in M5 computes from threshold/ratio. |
| 6 | Limiter | ceiling | -1.0 dBFS | -6…0 | |
|   |  | lookahead | 5 ms | fixed | Delay line of 240 samples at 48 k. The only algorithmic latency in the chain. |
|   |  | release | 60 ms | 10…500 | |
| 7 | Output gain | gain | 0 dB | -24…+12 | Post-limiter trim; a hard clip at 0 dBFS follows as a safety net. |

**Limiter design.** Per sample compute the gain needed `g = min(1, ceil / |x|)`.
Run a sliding-window minimum over the lookahead length (monotonic deque, O(1)
amortised, preallocated), then smooth the result with an attack time equal to
the lookahead and the user's release. Apply to the *delayed* signal. The
delayed peak arrives exactly as the gain has finished falling, so it is a true
brickwall for the ceiling. The hard clip after it exists only to guarantee the
invariant under floating-point edge cases.

**Expander hold.** Standard branching detector plus a hold counter: when the
detector starts falling, hold the gain for `hold` ms before applying the release
curve. This is what stops syllable gaps from fluttering.

**Smoothing times.** Gains and thresholds: 10 ms. Filter frequencies and Qs:
30 ms (recomputed per block). Ratios: 10 ms. Bypass: 5 ms crossfade between
dry and wet, not a hard switch.

### 4.3 Built-in presets

| Preset | Intent |
|---|---|
| **Flat** | Every stage bypassed. For A/B. |
| **Headset clean-up** | The defaults above. |
| **Podcast** | Softer: expander -50/1:2, comp 2.5:1 at -20, presence +1.5, de-esser lighter. |
| **Broadcast** | Harder: HPF 100 Hz, comp 4:1 at -16 with +8 makeup, presence +3.5, limiter -0.5. |
| **Late night** | For quiet speaking: input +6, expander -55, comp 4:1 at -24, makeup +10. |

User presets are saved as JSON in `app_data_dir/presets/<name>.json`.

---

## 5. Tech stack (versions as of 2026-10-09)

| Layer | Choice | Version | Why |
|---|---|---|---|
| Shell | Tauri | 2.12 | Same as the user's other apps. |
| Frontend | Vanilla TypeScript + Vite, no framework | TS 7, Vite 8 | Mirrors `wonkitch`. The UI is a settings panel plus meters; a framework buys nothing. |
| Audio I/O | `cpal` | 0.18 | WASAPI shared mode, device enumeration, F32 streams. |
| Ring buffer | `ringbuf` | 0.5 | Lock-free SPSC with `HeapRb`. |
| DSP | hand-written in a `dsp` crate | — | ~600 lines total; owning it means we understand every artifact. `biquad` crate is a fallback reference, not a dependency. |
| Allocation guard | `assert_no_alloc` | latest | Debug-only panic if the audio thread allocates. |
| WAV I/O | `hound` | 3.5 | Offline harness and tests only. |
| Resampling | `rubato` | 5.0 | M6, only if a device isn't 48 k or drift correction proves audible. |
| Denoise (optional) | `nnnoiseless` | 0.5 | M7, pure-Rust RNNoise, 10 ms frames. |
| Persistence | `serde` + `serde_json` | 1 | Same `preferences.json` pattern as wonkitch. |
| Tauri plugins | `single-instance`, `autostart`, `process`, `updater`, `opener` | 2.x | Tray is core Tauri (`tray-icon` feature). |
| Tests (TS) | `node --test` + `tsx` + `jsdom` + `@tauri-apps/api/mocks` | — | Copied from wonkitch. |
| Tests (Rust) | inline `#[cfg(test)]` + `cargo test` in `dsp` | — | Synthetic-signal assertions. |

---

## 6. Repository layout

```
mixer/
├── PLAN.md                        this file
├── README.md, docs/DEVELOPMENT.md, docs/RELEASING.md   (wonkitch structure)
├── package.json, tsconfig.json, vite.config.ts          (copied from wonkitch, renamed)
├── index.html                     all static markup incl. stage cards skeleton
├── src/
│   ├── main.ts                    bootstrap only: init modules in order, nothing else
│   ├── dom.ts                     element<T>() helper (from wonkitch)
│   ├── ipc.ts                     call<T>(cmd,args) wrapper + typed listen() helpers + error cleanup
│   ├── titlebar.ts                drag / dblclick-maximise / 8 resize handles / controls (port)
│   ├── chain-schema.ts            DATA-DRIVEN stage + parameter table (id, label, unit, min, max, step, default, format)
│   ├── stage-cards.ts             renders chain-schema into cards; wires input→set_param; bypass buttons
│   ├── devices.ts                 device selects, start/stop, status pill
│   ├── meters.ts                  canvas meters at 30 Hz with peak hold and GR bars
│   ├── preferences.ts             load / debounce-save / flush-on-close (port of wonkitch pattern)
│   ├── presets.ts                 built-in + user presets, select + save-as
│   ├── format.ts                  "90 Hz", "-18.0 dB", "1:2.5", "120 ms"
│   └── styles.css                 tokens + components (section 7)
├── tests/
│   ├── chain-schema.test.js       every param has default within range, unique ids, formatters
│   ├── stage-cards.test.js        JSDOM + mockIPC: slider input → set_param call shape
│   ├── preferences.test.js        JSDOM + mockIPC roundtrip (port)
│   └── presets.test.js
├── crates/
│   └── dsp/                       pure DSP, no tauri, no std::sync
│       ├── src/{lib,biquad,smoother,detector,gain_computer,delay}.rs
│       ├── src/stages/{highpass,expander,deesser,eq,compressor,limiter}.rs
│       ├── src/chain.rs           Chain { stages, params snapshot, process_block() }
│       ├── src/params.rs          ParamId enum + Params (AtomicU32 table) + ParamSpec table shared with TS via a generated JSON
│       ├── examples/offline.rs    wav in → wav out, prints per-stage GR stats
│       └── tests/*.rs             sine/noise/burst assertions
└── src-tauri/
    ├── Cargo.toml                 depends on ../crates/dsp
    ├── tauri.conf.json            frameless 980×720, min 820×600, not maximised, strict CSP
    ├── capabilities/default.json
    └── src/
        ├── main.rs
        ├── lib.rs                 Builder: plugins, manage(EngineHandle), manage(PreferencesState), commands, tray, RunEvent::Exit → engine shutdown
        ├── commands.rs            #[tauri::command] fns, all Result<T, String>
        ├── engine/mod.rs          spawn engine thread; EngineHandle { tx: Sender<EngineCommand>, params: Arc<Params>, status: Arc<Mutex<EngineStatus>> }
        ├── engine/devices.rs      enumerate, stable ids (name + direction), match saved id → Device
        ├── engine/streams.rs      build input/output streams, ring buffer, drift control, xrun counting
        ├── engine/meters.rs       atomics + 33 ms snapshot → emit
        ├── preferences.rs         AppPreferences (serde default + normalize + version), PreferencesState
        └── presets.rs
```

Cargo workspace at the repo root with members `src-tauri` and `crates/dsp`.
The `dsp` crate must build and test with `cargo test -p dsp` in under 10 s with
no Tauri involved; that is where most iteration happens.

**Single source of truth for parameters.** `crates/dsp/src/params.rs` holds
the `ParamSpec` table (id, stage, min, max, default, unit). A small build step
(`cargo run -p dsp --example dump-schema > src/chain-schema.json`) emits it for
the frontend, and a test asserts the checked-in JSON matches. This avoids
wonkitch's triplicated preferences schema.

---

## 7. UI design (derived from wonkitch, new palette)

_Superseded 2026-10-09: the vertical card layout described here was built,
rejected, and removed. The UI is being designed separately; see
[docs/UI-CONTRACT.md](docs/UI-CONTRACT.md) for what the design has to work
with. Sections 7.1, 7.2 and 7.4 still describe the shared visual language and
the meter behaviour._

### 7.1 What is copied verbatim

- **Window**: `decorations: false`, custom 40 px topbar, `data-tauri-drag-region`
  plus the mousedown handler that ignores interactive children, double-click to
  maximise, eight 5 px edge / 10 px corner resize handles via
  `startResizeDragging`, 43×39 flat caption buttons with the red close hover.
- **Type**: Segoe UI body. Sizes 12 px (labels), 11.5 px (nav/descriptions),
  10.5 px (helper text). Consolas for every small caps label: 8.5–9.5 px with
  0.05–0.12 em tracking, caps typed in the markup, not via `text-transform`.
  Titles 13 px/600. Page title 21 px/600 with -0.02 em.
- **Density**: gaps 4/6/8/12 px, paddings 9–12 px, cards radius 7 px, inputs
  and buttons radius 4–5 px, dialogs 8 px. 1 px hairlines everywhere, shadows
  only on floating elements. Settings rows `min-height 54px; padding 9px 12px`,
  label stack left, control right, bottom hairline except last.
- **Controls**: native `input[type=range]` and `checkbox` re-skinned with
  `accent-color`, `.range-control` row with the mono `<output>` readout at the
  right (43 px wide, right-aligned). `.secondary-button`, `.primary-button`,
  `.icon-button` 26×26, `.live-pill` 18 px, `.status-dot` 5 px with glow.
- **Motion**: 120 ms ease on buttons, 150 ms on focus rings, no spring/bounce.
  `body.reduced-motion` kill switch bound to a checkbox.
- **Scrollbars**: thin, `--line-bright` thumb, no track.
- **Icons**: inline SVG, 12–14 px, `stroke="currentColor"` 1.2 px. No emoji,
  no icon font.

### 7.2 Palette (different hues, same structure)

Keep the four-surface-plus-two-hairline structure and token names so the CSS
ports cleanly, change the values. Proposed, to be confirmed by the user:

| Token | Role | Proposed |
|---|---|---|
| `--ink` | window background | `#0c0b09` (warm near-black) |
| `--panel` | bars, card headers | `#121110` |
| `--panel-raised` | hover, inputs | `#1a1917` |
| `--line` / `--line-bright` | hairlines | `#201e1b` / `#2d2a26` |
| `--text` / `--muted` / `--faint` | type | `#ece9e4` / `#a39e95` / `#625d55` |
| `--accent` / `--accent-bright` / `--accent-soft` | active, focus, readouts | teal `#1fb8a6` / `#3fd3c1` / `rgba(31,184,166,.14)` |
| `--signal` | meter body | `#3fd3c1` (same as accent-bright) |
| `--hot` | meter -12…-3 dBFS, gain reduction | amber `#f0b23c` |
| `--clip` | meter > -3 dBFS, errors, close hover | `#ff4655` |

Meters are the only place three semantic colours are needed; everywhere else
the single teal accent is used as sparingly as wonkitch uses its purple.

### 7.3 Layout

Single window, no settings dialog: the whole app is the control panel.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ ◧ MIXER   ● LIVE  HyperX → CABLE Input   31 ms   0 xruns          ─  □  ✕   │ 40px topbar
├──────────────────┬───────────────────────────────────────────────────────────┤
│ INPUT            │  SIGNAL CHAIN                         PRESET [Headset ▾] │
│ [HyperX mic   ▾] │ ┌─ 1  HIGH-PASS ───────────────────────────── ● [BYP] ─┐ │
│ OUTPUT           │ │ Cutoff                   ────────●────   90 Hz       │ │
│ [CABLE Input  ▾] │ └──────────────────────────────────────────────────────┘ │
│                  │ ┌─ 2  EXPANDER ──────────── GR ▂▂▂▂▂▂  -6.2 ● [BYP] ─┐ │
│ [ ■ STOP       ] │ │ Threshold                ──────●──────  -45 dB      │ │
│                  │ │ Ratio                    ───●─────────  1:2.5       │ │
│ IN   ▐▐▐▐▐▐▐▐▁▁  │ │ Range                    ...                        │ │
│ OUT  ▐▐▐▐▐▐▐▁▁▁  │ └──────────────────────────────────────────────────────┘ │
│ -60 -40 -20 -6 0 │ ┌─ 3  DE-ESSER ───────────────────────────────────── … │
│                  │ ┌─ 4  EQ ────────────────────────────────────────── … │
│ ENGINE           │ ┌─ 5  COMPRESSOR ────────────────────────────────── … │
│ 48 000 Hz  480 f │ ┌─ 6  LIMITER ───────────────────────────────────── … │
│ CPU 1.2 %        │                                                         │
├──────────────────┴───────────────────────────────────────────────────────────┤
│ RESET DEFAULTS                                                    saved     │ 38px footer
└──────────────────────────────────────────────────────────────────────────────┘
      220px                              1fr
```

- **Left rail** (220 px, `--panel`, right hairline): device selects (wonkitch
  select style, 29 px), primary START/STOP button (30 px, mono caps), two
  horizontal peak meters with a shared dBFS scale in 8 px mono, an ENGINE group
  with mono readouts (sample rate, block size, measured latency, xruns, CPU).
- **Main column**: a `.settings-page-title`-style header row holding the mono
  kicker "SIGNAL CHAIN" and the preset select + save button at the right. Then
  six `.settings-group` cards in signal order. Card header = number, mono caps
  name, a 60 px inline gain-reduction bar for dynamics stages, status dot
  (active = accent, bypassed = faint), and a `BYP` pill toggle
  (`aria-pressed`). Card body = setting rows: label + 10.5 px helper text on
  the left, `.range-control` on the right. Rows are 48 px here (tighter than
  wonkitch's 54) because there are ~30 of them.
- **Footer**: ghost RESET DEFAULTS at left, save status at right, as wonkitch.
- **Keyboard / mouse**: sliders take arrow keys (step), Shift+arrow (10×),
  double-click readout resets to default, wheel over a slider nudges one step.
  Space toggles bypass on a focused card header.
- **Tray**: icon with LIVE/STOPPED state, menu: Start/Stop, Open, Quit.
  Closing the window hides to tray by default (preference). Single-instance
  focuses the existing window.

### 7.4 Meters

Canvas, drawn on each `engine:meters` event (30 Hz), devicePixelRatio aware.
Peak-hold marker holds 1 s then falls at 12 dB/s. Scale -60…0 dBFS, log in dB
but linear in pixels. Colour bands: signal to -12, amber to -3, red above.
A clip indicator latches red at the right end until clicked. GR bars draw
right-to-left in amber from 0 to -24 dB. When the window is hidden the engine
keeps running and the frontend just stops drawing.

---

## 8. IPC contract

Commands (snake_case names, camelCase JSON, every one returns `Result<T, String>`):

```ts
// devices
list_audio_devices(): { inputs: AudioDevice[]; outputs: AudioDevice[] }
  type AudioDevice = { id: string; name: string; isDefault: boolean; sampleRate: number; channels: number }
// engine lifecycle
start_engine({ inputId, outputId }): EngineStatus
stop_engine(): EngineStatus
get_engine_status(): EngineStatus
  type EngineStatus = { running: boolean; inputName?: string; outputName?: string; sampleRate?: number;
                        blockFrames?: number; latencyMs?: number; xruns: number; error?: string }
// parameters (hot path, ~ every slider input event; cheap: one atomic store)
set_param({ id: ParamId, value: number }): void
set_bypass({ stage: StageId, bypassed: boolean }): void
get_params(): Record<ParamId, number>                 // on load and after preset change
// persistence
get_preferences / save_preferences({ preferences }) / reset_preferences(): AppPreferences
list_presets(): PresetSummary[]        // built-ins + user
load_preset({ name }): Record<ParamId, number>         // also applies to engine
save_preset({ name }) / delete_preset({ name }): PresetSummary[]
// utilities
open_vb_cable_download(): void          // opener plugin, allowlisted URL
```

Events (backend → frontend; a deliberate departure from wonkitch, which polls):

```ts
"engine:state"  → EngineStatus          // on start/stop/error/device-change
"engine:meters" → { inPeak: number; outPeak: number; clipped: boolean;
                    gr: { expander: number; deesser: number; compressor: number; limiter: number } } // dB, 30 Hz
```

Frontend `ipc.ts` provides `call<T>(cmd, args)` that strips the `Error:`
prefix, and `subscribe(name, handler)` returning an unlisten; `main.ts` calls
the unlisteners on `beforeunload`.

---

## 9. Persistence

`%APPDATA%\com.chrismacdonaldw.mixer\preferences.json`, pretty JSON,
`#[serde(default, rename_all = "camelCase")]`, `version` field, `normalize()`
clamps every param to its `ParamSpec` range. Write via temp file + rename
(fixing wonkitch's non-atomic write).

```jsonc
{
  "version": 1,
  "devices": { "inputId": "…", "outputId": "…" },
  "autoStartEngine": true,
  "startMinimized": false,
  "closeToTray": true,
  "launchAtLogin": false,
  "reducedMotion": false,
  "activePreset": "Headset clean-up",      // or null when edited
  "params": { "hpf.cutoff": 90, "exp.threshold": -45, … },
  "bypass": { "hpf": false, "exp": false, … },
  "window": { "x": …, "y": …, "w": 980, "h": 720 }
}
```

Save path: slider `input` event → `set_param` immediately (engine) and
`scheduleSave()` with 300 ms debounce, generation counter, serialised queue,
`flushPendingSave()` on close request. Exactly the wonkitch pattern.

---

## 10. Milestones

Each milestone ends green on `npm test`, `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`.

_Status 2026-10-10: M0–M5 are in `main`. The stage UI from M3 was replaced by
the Claude Design panel (see `docs/UI-CONTRACT.md`). Calibration was pulled
forward as M6 because it is what makes the app useful without tuning by ear;
robustness, quality upgrades and release follow as M7–M9._

### M0 — Foundations (½ day)
- Install VB-Cable. In Windows Sound settings: set CABLE Input/Output to
  48 000 Hz 16-bit or 24-bit (both must match the mic), disable "audio
  enhancements" on the HyperX mic, make sure "Listen to this device" is off.
- `npm create tauri-app` (vanilla-ts) into `C:\dev\mixer`, convert to a Cargo
  workspace, add empty `crates/dsp`.
- Port from wonkitch: tsconfig, vite config, test script (as a glob), titlebar
  module, CSS reset + tokens (new palette), window-control markup, capability
  file, `.gitignore`, `docs/DEVELOPMENT.md` skeleton.
- `git init` and commit with the plan.
- **Done when**: `npm run tauri dev` shows a frameless, draggable, resizable
  empty window with the new palette and the wonkitch chrome.

### M1 — Passthrough with meters (1 day)
- `engine/` module: thread, `EngineCommand`, device enumeration with stable
  ids, stream building, ring buffer, drift control, xrun counter, meter
  atomics, 33 ms emit.
- Commands: list devices, start, stop, status. Events: state, meters.
- UI: left rail (device selects, START/STOP, two meters, engine readouts).
- Refuse mismatched sample rates with a readable error in the status pill.
- **Done when**: Discord's mic test, with CABLE Output selected, plays your
  voice; measured latency readout agrees with an Audacity dual-record test
  within 5 ms; 30 minutes of running shows 0 xruns and latency within 1 ms of
  its starting value (drift control works).

### M2 — DSP core, offline (1–2 days)
- `dsp` crate: biquad, smoother, detector, gain computer, delay, all six
  stages, `Chain`, `Params`, `ParamSpec` table, schema dump example.
- Tests (synthetic): HPF at 90 Hz attenuates a 45 Hz sine by ≥ 11 dB and
  passes 1 kHz within 0.1 dB; compressor static curve matches the formula at 5
  points; expander returns to unity above threshold and never exceeds `range`;
  limiter output never exceeds ceiling on a +6 dBFS burst and reaches ≥ 95 %
  gain within 3× release; de-esser reduces a 7 kHz burst more than a 1 kHz
  burst by ≥ 10 dB; a 2-minute silence through the whole chain produces no
  denormal slowdown (timed); parameter sweep over the full range produces no
  NaN/inf and no sample-to-sample jump > 0.1 (zipper test).
- `examples/offline.rs`: record 30 s of raw mic to WAV once (via the M1 engine
  with a "record raw" dev flag), then iterate on the chain by ear with
  `cargo run -p dsp --example offline -- in.wav out.wav`.
- **Done when**: tests pass and the offline render of the raw recording sounds
  right to the user with the default preset.

### M3 — Live chain and stage cards (1 day)
- Wire `Chain` into the output callback behind `Params`.
- `chain-schema.ts` generated; `stage-cards.ts` renders the six cards;
  `set_param` on input; bypass pills; GR bars in headers; readout formatting.
- **Done when**: every slider audibly changes the live signal with no clicks;
  bypass crossfades; GR bars move; CPU of the engine process stays under 3 %.

### M4 — Persistence and auto-start (½ day)
- Preferences on both sides with normalize + version; debounce save;
  restore params, bypass, devices on launch; `autoStartEngine`.
- **Done when**: kill the app, relaunch, it is LIVE with the same settings
  before the window finishes painting.

### M5 — Presets and control polish (1 day)
- Built-in presets, user presets, "edited" state, save-as dialog (native
  `<dialog>` in wonkitch style), reset defaults, double-click/wheel/Shift
  behaviour, auto-makeup toggle, 4th-order HPF toggle, reduced-motion toggle.
- TS tests for presets and cards.
- **Done when**: switching presets is click-free and persisted; all tests green.

### M6 — Calibrate (1–2 days)
- Engine: a lock-free capture tap on the raw input (pre-chain) drained by the
  worker thread; `begin_take` / `end_take` commands with a 60 s cap; takes
  held per role (silence, speech, loud) in a session store.
- `dsp::analysis`: frame peak statistics, activity detection against the
  measured noise floor, pitch tracking (normalised autocorrelation at 8 kHz),
  sibilance centre by sweeping the de-esser's own sidechain filter, a 24-band
  third-octave spectrum against the long-term average speech spectrum.
- `dsp::analysis::derive`: profile → every main-tier value plus the noise
  suppression switch, with a plain-English note per stage; detail-tier values
  untouched; `Tone` picks Neutral / Warm / Bright / Flat for the EQ only.
- UI: a wizard page designed from `docs/CALIBRATE.md` (prompts, live input
  level while recording, retake on clipping, tone choice, notes, Apply).
- **Done when**: the user records three takes, applies the result, and the
  live chain sounds right in Discord without touching a value; synthetic-voice
  tests pin pitch, sibilance, noise floor and every derived value's range.

### M7 — Robustness (1 day)
- Device unplug → engine stops cleanly, state event, UI offers restart;
  re-plug or default-device change → auto-restart if `autoStartEngine`.
- Tray icon + menu, close-to-tray, start minimised, single-instance,
  launch-at-login (autostart plugin).
- Resampler path for non-48 k devices (`rubato`), guarded by a test with a
  44.1 k fake config.
- First-run detection: no CABLE device found → inline notice with the
  download button and a 3-step setup card.
- **Done when**: unplugging and replugging the headset during a call recovers
  without relaunching; app survives a Windows audio service restart.

### M8 — Quality upgrades (optional, pick by ear)
- True-peak limiting (4× oversampled detector) if inter-sample clips show up.
- RNNoise landed early as the optional noise suppression stage (M5).
- Audition loop: play a recorded take through the live chain instead of the
  mic, so presets and tweaks are compared on identical material. Landed as a
  second chain on the same parameters, looping a take into the headphones
  while the mic keeps feeding the cable; see `docs/UI-CONTRACT.md`.
- "Monitor" toggle: also send the processed signal to the headphones, with a
  warning that you will hear ~35 ms of your own voice delayed.
- Spectrum strip under the EQ card (FFT in the engine thread from a meter
  ring, not in the callback).
- Stage reordering by drag (the screenshot's handles), if ever wanted.

### M9 — Release (½ day)
- Icons, NSIS installer, updater key, `latest.json`, `build-release.ps1`,
  README with the VB-Cable setup steps. All lifted from wonkitch's
  `scripts/` and `docs/RELEASING.md`.

Total: roughly 8–10 focused days to a shippable v1 (M0–M7, M9).

---

## 11. Testing and verification strategy

- **DSP**: synthetic-signal unit tests in `crates/dsp/tests` (section M2) run
  in seconds and gate every change. Property-style sweep over all params for
  NaN/inf and discontinuities.
- **Engine**: integration test spins the engine against cpal's default devices
  for 2 s and asserts streams start, meters tick and stop cleanly (skipped in
  CI without audio hardware via an env var).
- **Frontend**: `node --test` with JSDOM of the real `index.html` and
  `mockIPC`, asserting the `set_param` payloads produced by slider events, the
  preferences roundtrip, preset switching and formatter output.
- **Manual latency check** (documented in DEVELOPMENT.md): in Audacity, record
  two tracks at once, the raw HyperX mic and CABLE Output, clap once, measure
  the offset between transients. Target < 40 ms. Repeat after an hour to
  confirm no creep.
- **Manual quality check**: record 30 s raw once, keep it in `docs/samples/`
  (git-ignored), render through presets with the offline example whenever the
  DSP changes, listen for pumping, lisping (over-de-essing), chopped word
  endings (expander too fast), and dullness (wideband de-ess).

---

## 12. Risks and mitigations

| Risk | Mitigation |
|---|---|
| cpal `Stream` is `!Send`; can't live in Tauri state | Dedicated engine thread owns streams; everything else talks over a channel. Decided. |
| Clock drift between USB mic and VB-Cable | The ring is steered on the age of its oldest frame, read from capture timestamps, repeating or dropping one sample at a time; `rubato` fallback. |
| 10 ms shared-mode periods are the floor in cpal | The output opens through `IAudioClient3` when the device grants a period shorter than its default (2.7 ms on VB-Cable); hardware that only offers 10 ms stays on cpal. See Latency below. |
| VB-Cable adds its own buffer | Tune its "Max latency" in the VB-Cable control panel; document the value that works. |
| Allocation or lock on the audio thread sneaks in | `assert_no_alloc` in debug; code review rule: callbacks only touch `Chain`, ring, atomics. |
| Denormals after silence | FTZ/DAZ on the output thread; test times a 2-minute silence. |
| Device ids change between sessions | Id = direction + name; fall back to default device with a visible notice. |
| Windows "enhancements" or NVIDIA Broadcast silently in the path | Setup card tells the user to pick the raw HyperX device and disable enhancements; status pill shows the exact input name. |
| Parameter schema drift between Rust and TS | Generated `chain-schema.json` with a test that fails on mismatch. |
| User can't hear what the chain does | Flat preset for A/B, per-stage bypass with crossfade, GR meters, optional monitor path in M7. |

### Latency

Measured 2026-10-10 with the HyperX Cloud Alpha Wireless microphone (F32
mono 48 kHz) into CABLE Input (F32 stereo 48 kHz), default chain. The number
is the footer's `latencyMs`: capture instant of a sample to the instant the
cable reports playing it, from WASAPI stream timestamps, plus the chain's 5
ms lookahead. It ends at the cable's render side, so the cable's own buffer
and the reading app are extra.

| Build | Footer showed | Measured | Dropouts |
|---|---|---|---|
| Before (ring primed to two 1056-frame blocks, cpal output) | 60-71 ms (an estimate) | 94-97 ms | 1 per start (a capture glitch before audio flowed), none after |
| Ring steered to one packet plus one block, cpal output | 69 ms | 68-71 ms | 0 in 60 s |
| Low-latency `IAudioClient3` output at 128 frames | 27-36 ms | 26-37 ms | 0 in 60 s, idle and with all 16 cores busy |

Only VB-Cable grants a shorter period on this machine; the hardware outputs
(HyperX headphones, NVIDIA HDMI and Broadcast, the TV) offer only their 10 ms
default and stay on cpal. The headphones as the output measure about 60 ms;
the first version of the low-latency path took them too, at 10 ms with a
two-period allowance, and measured about 70 ms.

What is left, roughly: 10 ms microphone packets (its driver allows no shorter
shared-mode period); 7-8 ms of ring above that (two 2.7 ms output periods,
because the cable's driver wakes on a millisecond timer and sometimes asks
for two at once, plus a 2 ms margin); 5-13 ms from handing a block to the
cable until it reports consuming it, settling at start into one of two
phases; 5 ms limiter lookahead. Under 20 ms would need a microphone with
shorter periods; on this hardware the floor is about 22 ms even with no
margins.

---

## 13. Decisions needed from the user

1. **App name and identifier** (plan assumes `mixer`, `com.chrismacdonaldw.mixer`).
2. **Accent colour**: teal as proposed, or something else. Everything else in
   the palette follows from the wonkitch structure.
3. **Close-to-tray by default**: yes (plan assumes yes).
4. **Monitor path in v1 or later**: plan says M7.
5. **Row density**: 48 px rows as planned, or wonkitch's 54 px.

---

## 14. First commands (M0)

```powershell
# in C:\dev\mixer
npm create tauri-app@latest . -- --template vanilla-ts --manager npm
npm install
npm install -D jsdom tsx
cargo new --lib crates/dsp
# then: workspace Cargo.toml at root, port titlebar/css/tests from ..\wonkitch
npm run tauri dev
```
