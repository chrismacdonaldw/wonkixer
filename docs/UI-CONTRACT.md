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
  flag, estimated latency in ms (including the chain's own lookahead and any
  optional-stage delay), dropout count, and two maps keyed by stage for the
  dynamics stages: `gainReduction` in dB and `detectorDb`, the level each
  stage's detector saw, for drawing live level against a threshold.
- Engine state: running or not, the chosen input and output device names,
  sample rate, and an error message when something failed.
- Presets: five built-ins (Flat, Headset clean-up, Podcast, Broadcast, Late
  night) are read-only. User presets are one JSON file each under
  `%APPDATA%\com.chrismacdonaldw.wonkixer\presets`, with list, load, save,
  rename, delete (to the Recycle Bin), import and export through native
  pickers, and an open-folder command. A preset document is
  `{ values: { key: number }, bypassed: { stageKey: bool } }`; missing keys
  fall back to defaults.
- Preferences persist the chosen devices, auto-start, the current session
  values and the active preset reference, so the last state comes back on
  launch whether or not it was saved as a preset.
- Calibration: while the engine is live, the UI can record up to three raw
  takes (silence, speech, optional loud), then ask for an analysis that
  returns a voice profile, a complete set of values and one plain-English
  note per stage. Applying it is an ordinary `set_params`. The flow, prompts,
  minimum lengths and result shape are in [`CALIBRATE.md`](CALIBRATE.md).

## Commands and events

```ts
list_audio_devices(): { inputs: AudioDevice[]; outputs: AudioDevice[] }
start_engine({ inputId, outputId }): EngineState
stop_engine(): EngineState
get_engine_state(): EngineState
get_schema(): { stages: StageSpec[]; params: ParamSpec[] }
get_params(): ParamValues            // { values: Record<key, number>; bypassed: Record<stageKey, boolean> }
set_param({ id, value }): void
set_bypass({ stage, bypassed }): void
set_params({ values }): ParamValues

get_preferences(): Preferences
save_preferences({ preferences }): Preferences
list_presets(): { builtin: string[]; user: { name, modifiedMs }[] }
load_preset({ kind: "builtin" | "user", name }): ParamValues
save_user_preset({ name, values }): { name, modifiedMs }
rename_user_preset({ from, to }): { name, modifiedMs }
delete_user_preset({ name }): void
import_preset(): { name, modifiedMs } | null      // native open dialog
export_preset({ name, values }): string | null     // native save dialog, returns the path
open_presets_folder(): void

begin_take(): void                                 // engine must be running; 60 s cap
end_take({ role }): { role, seconds, peakDb, clipped }   // role: "silence" | "speech" | "loud"
list_takes(): TakeSummary[]
discard_takes(): void
analyze_takes({ tone }): { profile: VoiceProfile; values: ParamValues; notes: { stage, text }[] }
                                                   // tone: "flat" | "neutral" | "warm" | "bright"

event "engine:state"  -> EngineState
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
