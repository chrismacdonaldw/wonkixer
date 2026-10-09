# UI contract

What the frontend has to work with. This is the runtime surface the engine
exposes; the visual design is open. The first attempt (a vertical list of
sliders) was rejected and removed, so nothing in `src/` prescribes how the
chain should look. The window chrome, device pickers, level meters and
engine readouts from milestone 1 are in place and can be kept or redone.

## Facts the design must respect

- Eight stages in a fixed order: input trim, high-pass, expander, de-esser,
  EQ, compressor, limiter, output trim. The six in the middle can be bypassed.
- 32 parameters. Their ids, labels, units, ranges, defaults and steps are in
  [`chain-schema.json`](chain-schema.json), regenerated with
  `cargo run -p wonkixer-dsp --example dump_schema`. The frontend fetches the
  same data at runtime, so it should render from the schema, not from
  hard-coded lists.
- Units: `decibels`, `hertz`, `milliseconds`, `ratio`, `q`, `switch`. Scale
  `logarithmic` means frequency- and time-like parameters want log travel.
  The expander ratio reads as `1:x`; other ratios as `x:1`.
- Changing a parameter is instant and click-free; the engine smooths it. The
  UI can send a value on every drag event.
- Meters arrive 30 times a second: input peak dBFS, output peak dBFS, a clip
  flag, estimated latency in ms, dropout count, and gain reduction in dB for
  expander, de-esser, compressor and limiter.
- Engine state: running or not, the chosen input and output device names,
  sample rate, and an error message when something failed.
- Presets exist in the engine (Flat, Headset clean-up, Podcast, Broadcast,
  Late night) and whole parameter sets can be replaced at once. Persistence
  and user presets are milestone 4 and 5 work and will follow the design.

## Commands and events

```ts
list_audio_devices(): { inputs: AudioDevice[]; outputs: AudioDevice[] }
start_engine({ inputId, outputId }): EngineState
stop_engine(): EngineState
get_engine_state(): EngineState
get_schema(): { stages: StageSpec[]; params: ParamSpec[] }
get_params(): { values: Record<string, number>; bypassed: string[] }
set_param({ id, value }): void
set_bypass({ stage, bypassed }): void
set_params({ values }): ParamValues

event "engine:state"  -> EngineState
event "engine:meters" -> { inputPeakDb, outputPeakDb, clipped, latencyMs, dropouts,
                           gainReduction: { expander, deesser, compressor, limiter } }
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
