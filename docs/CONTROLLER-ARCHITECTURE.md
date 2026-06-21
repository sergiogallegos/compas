# compas — Controller architecture

> How compas supports hardware DJ controllers, and how to add one. The guiding principle: **the app
> is fully usable with mouse + keyboard alone; a controller is an additive layer.** Adding a new
> controller should be a *data* task (drop in a profile), not a recompile.

## Goals

- **Software-first.** Every function is reachable on-screen. Controllers enhance; they're never required.
- **Many devices, low contributor barrier.** A controller = a profile file. Contributors (or users)
  add devices without touching Rust.
- **One stable target vocabulary.** Controllers map onto the named **control bus**, so every device
  speaks the same language and a mapping written once keeps working as the UI evolves.
- **Two tiers of mapping.** Most controllers need only declarative bindings; advanced devices
  (jog modes, shift layers, banks, LED feedback) use the scripting layer.

## The layers (and where each lives today)

```
 physical device ─(MIDI/HID)→ input layer ─→ mapping/scripting ─→ control bus ─→ engine commands
                                                                       │
 device LEDs/faders ◀─(MIDI/HID out)── feedback ◀──────────────────────┘ (control values → MIDI)
```

1. **Control bus** — `compas-core::control`. Every tweakable engine value has a stable id
   (`"deck.0.gain"`, `"mixer.crossfader"`) and a **behavior curve** mapping engine value ⇄
   normalized `0..1` ⇄ MIDI `0..127`. This is the contract everything else targets.
2. **Declarative mapping** — `compas-core::mapping`. A serializable set of bindings
   (channel + note/CC → control id) resolved through the bus's behavior, with **soft-takeover** so a
   physically out-of-position knob doesn't jump the value when you switch layers/decks.
3. **Scripting** — `compas-script`. A sandboxed QuickJS runtime exposing `engine.set(id, value)` /
   `engine.log(...)` and an `onMidi(status, d1, d2)` hook, for device logic declarative bindings
   can't express. Scripts target the same control-bus ids.
4. **Input layer** — MIDI today (`midir`); **HID** is planned (`hidapi`) for pro controllers whose
   jog wheels / displays / hi-res controls aren't class-compliant MIDI.
5. **Feedback/output** — the bus's `to_midi` already converts a control's current value back to
   `0..127` for LED rings / motor faders; output bindings + an engine→controller channel send it.

## Controller profile format (proposed)

A profile is a JSON file — bundled with the app and/or dropped into the user's controller directory.
Conceptually:

```jsonc
{
  "id": "vendor-model-v1",          // stable slug
  "name": "Vendor Model",           // display name
  "ports": { "input": "Model MIDI", "output": "Model MIDI" }, // name hints for auto-connect
  "bindings": [
    { "channel": 0, "input": { "kind": "cc", "cc": 7 },  "control": "deck.0.gain", "soft_takeover": true },
    { "channel": 0, "input": { "kind": "note", "note": 36 }, "control": "deck.0.play" }
  ],
  "script": "optional-device-logic.js"  // only when bindings aren't enough
}
```

- `bindings` deserialize straight into `compas_core::mapping::Mapping`.
- `script`, when present, is loaded into a `compas_script::ScriptRuntime`; its `onMidi` handler runs
  for messages the static bindings don't claim (or for all messages, device's choice).
- A profile with **no** script is pure data — the common case.

## Loading & distribution

- **Bundled profiles** ship read-only with the app (a `controllers/` resource dir).
- **User profiles** live in a writable app-data `controllers/` dir and **override** bundled ones by
  id — so a user can tweak a mapping without forking. (This file-based override model is the standard
  way to let users adjust mappings without rebuilding.)
- At startup compas enumerates both dirs, matches connected MIDI/HID ports against profile `ports`
  hints (and offers a manual picker), and activates the chosen profile.

## Output / feedback

- Each control can have **output bindings** (control id → MIDI note/CC on the device). When the
  engine value changes, the control bus computes the `0..127` value (`Behavior::to_midi`) and the
  output layer sends it, lighting pads, drawing VU on rings, or moving motor faders.
- Scripts can also push feedback explicitly via the `engine` API (future: `engine.sendMidi(...)`).

## Soft-takeover & the learn editor

- **Soft-takeover** (in the mapping resolver) prevents value jumps when a knob's physical position
  differs from the software value after a layer/deck switch.
- A **guided learn editor** (UI) lets a user pick a control, wiggle a knob/pad, and capture the
  binding — writing a user profile with no JSON by hand. This complements (doesn't replace) authored
  profiles.

## Intellectual-property discipline (important)

compas's controller support is **independent and clean-room**:

- The MIDI/HID assignments of a device (which CC/note each knob/pad sends) are **facts about the
  hardware**, defined by the manufacturer — not copyrightable. Derive them from the manufacturer's
  **official MIDI implementation chart**, or by observing the hardware with a MIDI monitor.
- **Do not copy or translate another application's mapping files** (their XML/JS are creative,
  copyrighted work). Where a device needs clever behavior, understand the behavior and implement it
  in our own design.
- Profiles, scripts, and docs in this repo **must not name other DJ software**; describe the device
  and the behavior, not who else mapped it.

## How to add a controller (contributor workflow)

1. Get the device's MIDI implementation chart (manufacturer docs) or capture it with a MIDI monitor.
2. Write a profile JSON: map each physical control to a control-bus id (see
   `compas-core::control::Registry` for the full id list).
3. If the device needs logic beyond 1:1 bindings (shift layers, jog modes, LED patterns), add a
   small script using the `engine.*` API.
4. Drop it in the user `controllers/` dir to test live; open a PR to bundle it.
5. Keep it clean-room — derive from hardware facts, name no other software.

## Phased plan

1. **Profile model + loader** — JSON profile (de)serialization over `mapping::Mapping`, bundled +
   user-override dirs, a picker, and wiring the active profile's resolver into the MIDI input path.
2. **Starter MIDI profiles** — a handful of popular class-compliant controllers, derived clean-room
   from manufacturer charts.
3. **Output/feedback** — output bindings + the engine→controller send channel (LEDs, VU, motor faders).
4. **Scripting host wiring** — route unclaimed MIDI into the profile's `ScriptRuntime.on_midi`, apply
   the returned control updates through the bus; add `engine.sendMidi` for script-driven feedback.
5. **Guided learn editor** — in-app capture of bindings into a user profile.
6. **HID** — `hidapi` input/output for non-MIDI pro controllers (jog wheels, displays, hi-res).

> Foundations already in place: the typed control bus (`compas-core::control`), the declarative
> mapping + soft-takeover resolver (`compas-core::mapping`), and the sandboxed scripting runtime
> (`compas-script`). Phase 4 builds the profile/loader/feedback/HID layers on top.
