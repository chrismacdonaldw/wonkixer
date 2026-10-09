# wonkixer

wonkixer is a Windows desktop app that runs a broadcast-style voice chain on a
microphone in real time and exposes the result as a virtual microphone for
Discord, OBS, Zoom and anything else that takes a mic.

The chain is high-pass, downward expander, de-esser, EQ, compressor and limiter.
Every stage is bypassable and every parameter is live. The design goals, in
order, are no perceptible latency, no artifacts, then tone.

See [PLAN.md](PLAN.md) for the architecture and roadmap and
[docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for building and testing.

## Status

Milestone 0: application shell. The engine and signal chain are not wired yet.

## Requirements

- Windows 10 or Windows 11, x64
- Microsoft Edge WebView2 Runtime
- [VB-Audio Virtual Cable](https://vb-audio.com/Cable/) for the virtual microphone
