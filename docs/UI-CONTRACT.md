# UI contract

What the frontend has to work with. This is the runtime surface the engine
exposes; the visual design is open. The first attempt (a vertical list of
sliders) was rejected and removed, so nothing in `src/` prescribes how the
chain should look. The window chrome, device pickers, level meters and
engine readouts from milestone 1 are in place and can be kept or redone.

## Facts the design must respect

- Nine stages in a fixed order: input trim, high-pass, noise suppression,
  expander, de-esser, EQ, compressor, limiter, output trim. Everything between
  the two trims can be bypassed. Noise suppression is `optional`: it starts
  bypassed, shows up on the Add effect page, and adds `latencyMs` (10 ms) only
  while it is on. The engine keeps it running so switching it is a crossfade.
  It only works when the chain runs at 48 kHz: at any other `sampleRate` in
  the engine state it passes audio through untouched and adds no latency, so
  the UI should present it as unavailable rather than as on.
- Each stage carries `family` (level, filter, cleanup, dynamics, sibilance, eq)
  which picks its page layout, a `title`, a one-line `blurb`, and `bypassable`.
- 32 parameters. Their ids, labels, units, ranges, defaults and steps are in
  [`chain-schema.json`](chain-schema.json), regenerated with
  `cargo run -p wonkixer-dsp --example dump_schema`. The frontend fetches the
  same data at runtime, so it should render from the schema, not from
  hard-coded lists.
- Units: `decibels`, `hertz`, `milliseconds`, `ratio`, `q`, `switch`. Scale
  `logarithmic` means frequency- and time-like parameters want log travel.
  The expander ratio reads as `1:x`; other ratios as `x:1`.
- Each parameter has `tier` (`main` or `detail`); table order is page order,
  main settings first. `step` is the fine resolution (0.1 dB, 0.01 Q) and
  `coarse` the normal one (0.5 dB, 0.05 Q).
- Changing a parameter is instant and click-free; the engine smooths it. The
  UI can send a value on every drag event.
- Meters arrive 30 times a second: input peak dBFS, output peak dBFS, a clip
  flag, `latencyMs`, `dropouts`, and two maps keyed by stage for the
  dynamics stages: `gainReduction` in dB and `detectorDb`, the level each
  stage's detector saw, for drawing live level against a threshold.
- `latencyMs` is measured, not estimated: on every output block the engine
  takes the instant the microphone captured the block's first sample and the
  instant the output device says it will play it (both from the Windows
  audio clock), and adds the chain's own delay: the limiter's 5 ms
  lookahead, noise suppression's 10 ms while it is on, and about 2 ms for the
  resampler when one is running. The value is the latest block's, so it
  moves by a few tenths of a millisecond. For roughly the first 20 ms after a
  start, before audio flows, it is a rough estimate from buffer sizes. It
  stops where the output device takes the audio: VB-Cable's own buffer
  between CABLE Input and CABLE Output, and whatever Discord or OBS add when
  they read CABLE Output, are not included. With the HyperX headset into
  VB-Cable it reads 27-36 ms (the cable settles into one of two phases when
  the route starts); a device that has to use the fallback output below
  reads about 70 ms.
- `dropouts` counts audio lost after audio started flowing: the transfer
  between the two devices running dry or overflowing, the microphone
  reporting a gap, or the output device's buffer running dry. Glitches
  reported while a route is starting are not counted. It resets when a route
  starts.
- The output opens in a low-latency mode, at the shortest shared-mode period
  the device allows (2.7 ms on VB-Cable, 10 ms on most hardware), whenever
  the device mixes in 32-bit float and offers that mode. Otherwise it falls
  back to a standard 10 ms shared-mode stream. Nothing in the engine state
  says which; only `latencyMs` shows the difference.
- Engine state: running or not, the chosen input and output device names,
  sample rate, an error message when something failed, `waitingForDevices`,
  and `audition` (below).
- The chain runs at the output device's rate. When the input runs at a
  different rate (a 44.1 kHz microphone into a 48 kHz cable, say) the engine
  resamples the input on the way in, which adds about 2 ms; nothing is
  refused for a rate mismatch any more.
- Device supervision: after a successful start the engine remembers the
  route. If a stream dies (the headset is unplugged or switched off, Windows
  resets the device, the audio service restarts) the state turns to
  `running: false` with a plain-English `error` such as `The input device
  "Microphone (HyperX Cloud Alpha Wireless)" was disconnected.` and
  `waitingForDevices: true`. Every 2 s the engine checks whether both devices
  are back and restarts the route on its own, emitting a normal running
  state. A start that fails only because a device is not connected also
  waits for it: `start_engine` still rejects with `The input device is not
  connected.` (or output), but the state it leaves behind, in the event and
  from `get_engine_state`, has `waitingForDevices: true`, so after a rejected
  start ask for the state before showing it as stopped. `stop_engine` ends
  the wait; so does turning supervision off
  with `set_engine_supervision({ enabled: false })` (it is on by default and
  not persisted, so a UI that ties it to `autoStart` should send it on
  launch). `waitingForDevices` is what to show as "waiting for the headset";
  while it is true the input and output in the state are `null`, so name the
  devices from the pickers.
- Presets: five built-ins (Flat, Headset clean-up, Podcast, Broadcast, Late
  night) are read-only. User presets are one JSON file each under
  `%APPDATA%\com.chrismacdonaldw.wonkixer\presets`, with list, load, save,
  rename, delete (to the Recycle Bin), import and export through native
  pickers, and an open-folder command. A preset document is
  `{ values: { key: number }, bypassed: { stageKey: bool } }`; missing keys
  fall back to defaults.
- Preferences persist the chosen devices, auto-start, the current session
  values and the active preset reference, so the last state comes back on
  launch whether or not it was saved as a preset. Each device id is saved
  with the name it had (`inputName`, `outputName`; `null` in files written
  before names were kept), so a picker can still show a saved device that is
  not connected at launch. They also carry
  `launchAtLogin` and `startMinimized`, which belong to the backend:
  `save_preferences` keeps their stored values whatever the frontend sends,
  and they change only through `set_launch_at_login` (which also registers or
  removes the Windows startup entry) and `set_start_minimized`. Both return
  the full preferences.
- Window and process: the app lives in the tray. Closing the window (the
  title bar close button, Alt+F4) hides it; the engine keeps running. The
  tray icon opens the window on a left click, and its menu has Show wonkixer,
  Start engine / Stop engine (follows the engine state; it reads Stop while
  waiting for devices, and Start uses the devices saved in preferences) and
  Quit, which shuts the engine down and exits. Launching a second copy only
  brings the first window back. The window is created hidden and the backend
  shows it at startup unless `startMinimized` is set, in which case the app
  starts in the tray with the page loaded and auto-start running as usual.
  The page is never torn down by a close, so it does not need to flush on
  close beyond its normal debounced save, and it must not call
  `window.destroy()` (the capability does not grant it).
- First run: `get_setup_status()` reports whether VB-Cable is installed,
  matched by endpoint name: a "CABLE Input" output (what the engine renders
  to) and a "CABLE Output" input (what Discord and OBS record from). Either id
  is `null` when its endpoint is missing; `cableInstalled` needs both. The
  device counts help tell "no cable" from "no audio devices at all".
  `open_cable_download()` opens the VB-Cable download page
  (https://vb-audio.com/Cable/) in the default browser.
- Calibration: while the engine is live, the UI can record up to three raw
  takes (silence, speech, optional loud), then ask for an analysis that
  returns a voice profile, a complete set of values and one plain-English
  note per stage. Applying it is an ordinary `set_params`. The flow, prompts,
  minimum lengths and result shape are in [`CALIBRATE.md`](CALIBRATE.md).
- Audition: while the engine is live, `start_audition({ role, outputId })`
  plays one recorded take (normally `speech`) on repeat through a second copy
  of the chain into a playback device, `outputId` or the Windows default
  output when it is `null`. The second chain reads the same parameters as the
  live one, so `set_param`, `set_bypass`, `set_params`, a preset load,
  COMPARE and HEAR DRY are heard on the loop at once. The microphone-to-cable
  route keeps running untouched and the loop never reaches the cable, so
  calls keep hearing the microphone. The take is the raw recording at the
  chain's rate; it starts from the top, with its ends faded over 15 ms so the
  loop point does not click, and a playback device at another rate gets it
  converted once before it starts. Its own latency does not matter and is not
  measured. While it plays the engine state carries
  `audition: { role, output: { id, name } }`, otherwise `null`. Calling
  `start_audition` again while it plays switches the take or the device in
  one step. It stops on `stop_audition`, on `begin_take` (a new recording
  never picks up the loop), whenever the engine stops, restarts or loses a
  device, and when the playback device goes away; in that last case the state
  just returns to `audition: null` with no `error`, since the route is fine.
  The loop plays its own copy of the take, so `discard_takes` or a new
  `end_take` does not change what is already playing. The meters keep
  describing the live microphone route, not the loop. It refuses with a
  sentence to show as is: `Start the engine before playing a take.`,
  `There is no speech take yet. Record one first.` (or silence, loud),
  `The playback device is not connected.`,
  `Windows has no default playback device. Pick one to listen on.`, and, for
  a CABLE Input or the route's own output device,
  `That device feeds your calls, so everyone would hear the loop. Pick your headphones.`
  Device format problems read like the route's, with "playback" as the role.

## Commands and events

```ts
list_audio_devices(): { inputs: AudioDevice[]; outputs: AudioDevice[] }
start_engine({ inputId, outputId }): EngineState
stop_engine(): EngineState                         // also ends waiting for devices
get_engine_state(): EngineState
set_engine_supervision({ enabled }): EngineState   // automatic recovery, on by default
get_setup_status(): { cableInstalled, cableInputId: string | null,
                      cableOutputId: string | null, inputs: number, outputs: number }
open_cable_download(): void                        // VB-Cable download page in the browser
get_schema(): { stages: StageSpec[]; params: ParamSpec[] }
get_params(): ParamValues            // { values: Record<key, number>; bypassed: Record<stageKey, boolean> }
set_param({ id, value }): void
set_bypass({ stage, bypassed }): void
set_params({ values }): ParamValues

get_preferences(): Preferences
save_preferences({ preferences }): Preferences     // keeps launchAtLogin and startMinimized
set_launch_at_login({ enabled }): Preferences      // updates the Windows startup entry too
set_start_minimized({ enabled }): Preferences
list_presets(): { builtin: string[]; user: { name, modifiedMs }[] }
load_preset({ kind: "builtin" | "user", name }): ParamValues
save_user_preset({ name, values }): { name, modifiedMs }
rename_user_preset({ from, to }): { name, modifiedMs }
delete_user_preset({ name }): void
import_preset(): { name, modifiedMs } | null      // native open dialog
export_preset({ name, values }): string | null     // native save dialog, returns the path
open_presets_folder(): void

begin_take(): void                                 // engine must be running; 60 s cap;
                                                   //   stops the audition
end_take({ role }): { role, seconds, peakDb, clipped }   // role: "silence" | "speech" | "loud"
list_takes(): TakeSummary[]
discard_takes(): void
analyze_takes({ tone }): { profile: VoiceProfile; values: ParamValues; notes: { stage, text }[] }
                                                   // tone: "flat" | "neutral" | "warm" | "bright"
start_audition({ role, outputId }): EngineState    // outputId null: the Windows default output;
                                                   //   again while playing switches take or device
stop_audition(): EngineState

event "engine:state"  -> EngineState                // { running, input, output, sampleRate,
                                                   //   error, waitingForDevices,
                                                   //   audition: { role, output: { id, name } } | null }
event "engine:meters" -> { inputPeakDb, outputPeakDb, clipped, latencyMs, dropouts,
                           gainReduction: Record<stageKey, number>,
                           detectorDb: Record<stageKey, number> }
```

Typed wrappers for all of this live in `src/engine.ts`; `src/param-scale.ts`
maps values to and from a 0..1000 control position with log or linear travel;
`src/format.ts` formats values per unit.

## Reference

- Visual language to match: the sibling repo `../wonkitch` (spacing, type,
  density, chrome) with this project's own palette. Tokens are at the top of
  `src/styles.css`.
- Hardware context: a headset microphone, one person, used while gaming or on
  calls. The user glances at it to confirm it is live and the levels are
  healthy, and occasionally tunes the sound by ear.
