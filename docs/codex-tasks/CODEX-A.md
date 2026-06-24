# CODEX-A — Move audio I/O device controls into the Settings panel

**Owner:** Codex (GPT-5.5)
**Reviewer:** Claude (do NOT push to `main`; commit on a branch or leave staged for review)
**Target version:** v0.2.0
**Status:** TODO

## Context

Compás DJ currently renders every audio input/output device control **inside the mixer column**
(`apps/compas-dj/frontend/src/components/Mixer.tsx`). The maintainer wants the *device-selection*
plumbing moved out of the always-visible mixer and into the existing **Settings** modal
(`apps/compas-dj/frontend/src/components/SettingsPanel.tsx`), so the main layout stays focused on
performance controls. Quick performance controls (ON/OFF + level knobs) may stay in the mixer; only
the **device pickers** and their setup must move to Settings.

## Scope — what to move

In `Mixer.tsx` these sub-components currently own a device `<select>` dropdown:

- **`Aux`** (mic / line-in) — device picker (`list_input_devices`)
- **`Phones`** (headphone / cue output) — device picker
- **`Booth`** (booth output) — device picker
- **Master output device** — if a picker exists for it, move it too (verify; it may be implicit/default)

Move the **device-picker dropdowns** (and any "rescan devices" affordance) into a new
**"Audio devices"** section of the Settings panel. Keep in the mixer strip: the ON/OFF toggle and the
level/gain knobs (CUE/MASTER blend, PHONES, BOOTH, AUX gain) and the live-BPM lock indicator — those
are performance controls, not setup.

## Files you may edit (and ONLY these — avoid collisions)

- `apps/compas-dj/frontend/src/components/SettingsPanel.tsx` — add the "Audio devices" section/tiles
- `apps/compas-dj/frontend/src/components/Mixer.tsx` — remove the device `<select>`s, keep ON/OFF + knobs
- `apps/compas-dj/frontend/src/styles.css` — any styling for the new Settings section (you already own this file)
- (If a small shared hook helps, you may add a new file under `src/hooks/` — but do NOT edit `useDeck.ts`.)

**DO NOT TOUCH:** `WaveformZone.tsx`, `Deck.tsx`, `App.tsx` layout structure, or any Rust/`src-tauri`
files. Claude is actively working in `WaveformZone.tsx` and the deck layout — editing those will
collide. The device IPC commands already exist (`list_input_devices`, the cue/booth/output device
setters via the `useAux`/`Phones`/`Booth` props) — reuse them, don't add new IPC.

## How SettingsPanel works today

`SettingsPanel.tsx` is a modal opened from the title bar. It renders a `.settings-grid` of
`.settings-tile` buttons (High contrast, Synth, MIDI map, Sampler, Controllers). Add a new labelled
**"Audio devices"** sub-section above or below the grid containing the relocated pickers. The panel
receives props from `App.tsx`; if you need the aux/phones/booth device state in Settings, thread the
existing hook objects (`aux`, `cue`/phones, `booth`) through as props — `App.tsx` already constructs
them and passes them to `Mixer`. **Adding a prop to SettingsPanel is fine; restructuring App's layout
is not** — just pass the already-existing objects through.

## Acceptance criteria

1. The mic, headphone/cue, and booth **device dropdowns no longer appear in the mixer column** — they
   live in the Settings modal under an "Audio devices" heading.
2. ON/OFF toggles and level knobs **remain in the mixer** and still work.
3. Selecting a device in Settings still routes correctly (same IPC calls as before — verify the picker
   still calls the same setters).
4. `cd apps/compas-dj/frontend && npx tsc --noEmit` is clean.
5. `cd apps/compas-dj/frontend && npx vite build` is clean.
6. Commits are **scoped to the files listed above** (so they rebase cleanly over Claude's work).

## When done

Commit on a branch (e.g. `codex/io-to-settings`) or leave the changes staged, and report back with a
one-paragraph summary of what moved + confirmation that `tsc` and `vite build` pass. Claude will
review before anything merges to `main`.
