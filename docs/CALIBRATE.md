# Calibrate — design brief

A guided flow that measures the user's voice and room, then writes every main
setting of the chain for them. It exists so a person who knows nothing about
audio never has to tune by ear. The engine side is built; this brief is the
input for the page design, in the same language as the main panel (grey
tokens, type scale, density and chrome from `src/styles.css`).

## The flow

1. **Entry.** A `CALIBRATE` action, reachable from the preset bar or the
   header. It needs the engine LIVE; if it is stopped the first step offers to
   start it with the current devices.
2. **Three takes, one after another.** Each take is a short recording of the
   raw microphone, before any processing.

   | Take    | Prompt to the user                                        | Minimum | Suggested |
   |---------|-----------------------------------------------------------|---------|-----------|
   | silence | Stay quiet. Let the room and the PC be heard.             | 1.5 s   | 5 s       |
   | speech  | Read the passage the way you normally talk on a call.     | 4 s     | 20–30 s   |
   | loud    | Read the same lines as if calling to someone across the room. Optional. | 2 s | 8 s |

   While a take records, the page shows the live input level (the existing
   `inputPeakDb` meter), elapsed time, and a Stop. Takes cap at 60 s. The
   reading passage should be sibilant-rich; the first paragraph of the
   Rainbow Passage (Fairbanks, public domain) works:

   > When the sunlight strikes raindrops in the air, they act as a prism and
   > form a rainbow. The rainbow is a division of white light into many
   > beautiful colors. These take the shape of a long round arch, with its
   > path high above, and its two ends apparently beyond the horizon.

   Each finished take comes back as a summary: duration, peak level, and a
   `clipped` flag. A clipped take should be shown as a warning with a retake
   button, not a hard stop.
3. **Tone choice.** Four options before analysis: `Neutral` (default), `Warm`,
   `Bright`, `Flat`. Only the EQ differs between them; everything else is
   measured.
4. **Result.** Analysis runs in well under a second and returns three things:
   the measured voice profile, the full set of values, and one plain-English
   note per stage explaining the decision. The page shows the notes, offers
   `Apply` (writes the values to the live chain through `set_params`, marks
   the session EDITED like any other change, so undo and Save as preset work
   as usual) and `Recalibrate`. A compare affordance is welcome: the existing
   COMPARE and HEAR DRY controls already do before/after on the live signal.
5. **Errors in plain words.** Analysis refuses with a sentence the page can
   show directly: a take too short, no speech found, the silence take was not
   quiet, or a dropped recording. Each maps to redoing one take.

## What the engine provides

```ts
begin_take(): void                       // engine must be running
end_take({ role }): TakeSummary          // role: "silence" | "speech" | "loud"
list_takes(): TakeSummary[]
discard_takes(): void
analyze_takes({ tone }): CalibrationResult   // tone: "flat" | "neutral" | "warm" | "bright"

TakeSummary = { role, seconds, peakDb, clipped }
CalibrationResult = {
  profile: {
    sampleRate, noiseFloorDb, speechSeconds, clipped,
    speech: { p10, p50, p90, max },            // peak dBFS of normal speech
    loud:   { p10, p50, p90, max } | null,
    pitchLowHz, pitchMedianHz,                 // null if unvoiced
    sibilance: { frequencyHz, levelDb, vowelLevelDb } | null,
    spectrum: { frequencyHz, levelDb, referenceDb }[]   // 24 third-octave bands, both anchored to 400–2000 Hz
  },
  values: ParamDocument,                       // ready for set_params
  notes: { stage, text }[]                     // one per stage, in chain order
}
```

The profile is there for optional visuals: the spectrum against the reference
speech curve, the noise floor against the quietest words, the sibilance
centre. None of it is required for the flow to work; the notes alone carry it.

## What calibration decides

| Measured                         | Written                                           |
|----------------------------------|---------------------------------------------------|
| Normal speech peak level         | Input trim, so speech peaks sit at -12 dBFS       |
| Lowest pitch                     | High-pass cutoff, 0.75 × pitch, 50–150 Hz          |
| Noise floor vs quietest words    | Expander threshold; noise suppression on or off   |
| Sibilance centre and level       | De-esser frequency and threshold                  |
| Spectrum vs reference, plus tone | EQ gains and the low-mid / presence frequencies   |
| Quiet-to-loud spread             | Compressor ratio, threshold and makeup            |
| Predicted peaks, loud take       | Output trim, kept under the limiter               |

Detail-tier parameters (attack, release, hold, knee, limiter) are left as the
user has them.
