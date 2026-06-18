# Changelog

All notable changes to compas are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/); the project uses [SemVer](https://semver.org/).

## [Unreleased]

### Added
- **Local dual-deck engine (Phase 1):** in-RAM decode (symphonia), cubic-interpolated
  fractional play-head (instant seek + varispeed), per-deck 3-band EQ, HPF/LPF filter, gain,
  equal-power crossfader, master, lock-free audio thread with reclaim ring.
- **Analysis:** BPM (spectral-flux onset → autocorrelation), beatgrid (beat phase), and musical
  key (chromagram → Krumhansl–Schmuckler, Camelot).
- **Tempo SYNC:** one-shot beat-tempo match (per-deck + title-bar).
- **Beat loops:** manual IN/OUT + 4/8/16-beat grid-snapped loops (RT-safe play-head wrap), with a
  loop region drawn on the waveform; **hot cues** (set/jump/clear).
- **Jog-wheel scratch:** the platter is a draggable, spinning disc — dragging drives the
  audio-thread read-rate from angular velocity (forward + reverse scrub, hold), independent of
  transport. Engine `SetScratch` command + `deck_scratch` IPC; the disc tracks the hand 1:1.
- **FX rack — echo/delay:** RT-safe stereo `Delay` (pre-allocated ring buffer, fractional
  read with one-pole time-glide for tape-style pitch bend, feedback + wet/dry). Per-deck insert
  (post-EQ) via `SetDeckEcho` / `set_deck_echo`; UI is a beat-synced toggle (¼/½/1/2 beats) with
  a single DEPTH knob. Criterion bench added.
- **FX rack — reverb:** RT-safe Freeverb-style `Reverb` (8 parallel damped comb filters → 4
  series allpass diffusers per channel, sample-rate-scaled tunings, all buffers pre-allocated).
  Per-deck insert (post-echo) via `SetDeckReverb` / `set_deck_reverb`; UI is a toggle with SIZE
  and MIX knobs. Criterion bench added.
- **Local library:** add files (persisted), search, load to deck A/B (double-click / buttons),
  with load-progress feedback.
- **Performance UI:** dual decks, center mixer, scrolling zoom waveforms with beat grid, VU
  metering, library browser, frameless window with custom controls.
- **Spotify (Phase 2a):** Authorization Code + PKCE auth and live catalog search (control-only).
- Brand mark + app icons; landing-page website; CI, contributor docs.

### Changed
- **Tempo −/+ buttons** are now a **persistent fine trim** (±0.1% per click, moves the pitch
  fader) instead of a momentary 3% pitch-bend that gave no visual feedback — momentary bend now
  lives on the jog wheel.

### Notes
- Streaming decks are **control-only** by design — services don't expose decoded audio, so DSP
  is locked on them. True mixing is local-files-only.

[Unreleased]: https://github.com/sergiogallegos/compas/commits/main
