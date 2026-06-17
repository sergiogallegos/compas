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
- **Performance UI:** dual decks, center mixer, scrolling zoom waveforms with beat grid, VU
  metering, library browser, frameless window with custom controls.
- **Spotify (Phase 2a):** Authorization Code + PKCE auth and live catalog search (control-only).
- Brand mark + app icons; landing-page website; CI, contributor docs.

### Notes
- Streaming decks are **control-only** by design — services don't expose decoded audio, so DSP
  is locked on them. True mixing is local-files-only.

[Unreleased]: https://github.com/sergiogallegos/compas/commits/main
