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
- **Synth instrument + MIDI input:** a polyphonic synth (4 waveforms, ADSR, 16 voices) on the
  master bus, playable from an **on-screen keyboard**, the **computer keyboard**, or a **MIDI
  controller** (via `midir` — connect a device; its notes play the synth, knobs emit `midi:cc`).
  RT-safe (fixed-voice, no allocation). It's mixed into the master, so it's recordable too.
- **Auto-mix / transitions:** an **AUTO** toggle (auto-transition near track end) and a **MIX**
  button (transition now). A transition cues the incoming deck at its first downbeat, beat-syncs
  it to the live deck, starts it, and runs a 16-beat crossfade with a **bass swap** (hands the low
  end over cleanly), then stops the outgoing deck. Frontend orchestration over the existing
  sync/crossfader/EQ — no audio-thread changes.
- **Continuous beat-sync (tempo + phase):** SYNC is now a toggle that holds a follower deck
  locked to a master in the audio thread — a phase-locked loop rate-matches the beat rate and
  nudges the read rate (±8%, click-free) to null the beat-phase error continuously. Composes with
  key-lock/loops; respects manual grid nudges. `SetDeckSync`/`SetBeatgrid` + engine PLL.
- **Key-lock (master tempo):** change tempo without changing pitch, via a hand-rolled,
  RT-safe WSOLA time-stretcher in `compas-dsp` (overlapping Hann grains + waveform-similarity
  search, reads grains straight from the in-RAM buffer, no allocation on the audio thread —
  ~4% of a core per deck). Per-deck `KEY` toggle; `SetDeckKeylock` / `set_deck_keylock`.
- **Beat loops:** manual IN/OUT + 4/8/16-beat grid-snapped loops (RT-safe play-head wrap), with a
  loop region drawn on the waveform; **hot cues** (set/jump/clear).
- **Jog-wheel scratch:** the platter is a draggable, spinning disc — dragging drives the
  audio-thread read-rate from angular velocity (forward + reverse scrub, hold), independent of
  transport. Engine `SetScratch` command + `deck_scratch` IPC; the disc tracks the hand 1:1.
- **FX rack — echo/delay:** RT-safe stereo `Delay` (pre-allocated ring buffer, fractional
  read with one-pole time-glide for tape-style pitch bend, feedback + wet/dry). Per-deck insert
  (post-EQ) via `SetDeckEcho` / `set_deck_echo`; UI is a beat-synced toggle (¼/½/1/2 beats) with
  a single DEPTH knob. Criterion bench added.
- **FX rack — reverb:** RT-safe Schroeder/Moorer-style `Reverb` (8 parallel damped comb filters → 4
  series allpass diffusers per channel, sample-rate-scaled tunings, all buffers pre-allocated).
  Per-deck insert (post-echo) via `SetDeckReverb` / `set_deck_reverb`; UI is a toggle with SIZE
  and MIX knobs. Criterion bench added.
- **RT-load / underrun meter:** the title-bar indicator now shows real audio-thread load
  (processing time ÷ block budget) and counts real-time-budget overruns (xruns), replacing the
  hardcoded "RT OK".
- **Manual beatgrid-anchor edit:** nudge a deck's beatgrid (±5 ms) from the waveform to line it
  up with the audio; feeds both the grid overlay and beat-loop math.
- **Master recording:** one-click record of the master mix to a 32-bit-float stereo WAV. The
  audio thread taps the post-crossfader master into a lock-free ring; a writer thread streams it
  to disk and finalizes on stop (`start_recording` / `stop_recording`). RT-safe — no allocation
  or file I/O on the audio thread.
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
