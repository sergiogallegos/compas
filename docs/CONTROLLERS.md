# compas — controller support matrix

> Target list of hardware controllers to support, grouped by manufacturer. Each becomes a
> **profile** (`docs/CONTROLLER-ARCHITECTURE.md`) — a JSON binding set (+ optional script), derived
> **clean-room** from the manufacturer's MIDI/HID implementation chart or by observing the hardware.
> No other DJ software is named in profiles or code.

**Status:** ⬜ planned · 🔨 in progress · ✅ shipped. Bundled starters so far: Korg nanoKONTROL2,
Akai MPK Mini MK3, Akai LPD8 (see the per-manufacturer sections).

**Per-profile metadata.** When a profile is authored, it records the device's **manual** and
**manufacturer product page** links in the profile/PR (so the site can list them), the connection
(**MIDI** or **HID**), and the deck count it drives. We don't list URLs here speculatively — they're
added with the profile to keep them accurate.

**Sourcing.** Primary: the manufacturer's official MIDI implementation chart / product manual.
Secondary: observe the hardware with a MIDI monitor. The local reference codebase may be read to
*understand* a device's behavior, but no mapping files are copied or translated — we implement our
own. (See the IP section of `CONTROLLER-ARCHITECTURE.md`.)

---

## Akai
- ✅ **MPK Mini MK3** (bundled starter; 8 knobs CC 70–77 → deck gains + filters, pads notes 36–43 →
  cue/sync — Akai factory default map) · ✅ **LPD8** (bundled starter; 8 knobs CC 1–8 → deck gains +
  filters, 8 pads notes 36–43 → cue/sync — Program-1 factory default) · ⬜ MPD24

> The two Akai starters above are authored from each device's documented factory-default program
> (a hardware fact, not a copied mapping). Pad/program defaults vary by firmware revision and editor
> program, so treat the CC/note numbers as a starting point — use the **guided learn editor** to
> re-bind for your specific unit/program if a control doesn't respond. Manuals: Akai MPK Mini MK3 and
> LPD8 product pages / quickstart guides (akaipro.com).

## Allen & Heath
- ⬜ Xone:K1 · ⬜ Xone:K2 · ⬜ Xone:K3

## American Audio
- ⬜ Radius 2000 · ⬜ VMS2 · ⬜ VMS4 / 4.1

## Arturia
- ⬜ KeyLab Mk1

## Behringer
- ⬜ BCD2000 · ⬜ BCD3000 · ⬜ BCR2000 · ⬜ CMD Micro · ⬜ CMD MM-1 · ⬜ CMD STUDIO 4a · ⬜ DDM4000

## Denon
- ⬜ DN-HS5500 · ⬜ DN-SC2000 · ⬜ MC3000 · ⬜ MC4000 · ⬜ MC6000MK2 · ⬜ MC7000

## DJ-Tech
- ⬜ CDJ 101 · ⬜ DJM 101 · ⬜ iMix Reload · ⬜ Kontrol One · ⬜ MIX-101 · ⬜ Mixer One

## DJ TechTools
- ⬜ MIDI Fighter Classic · ⬜ MIDI Fighter Spectra · ⬜ MIDI Fighter Twister

## EKS
- ⬜ Otus

## Electrix
- ⬜ Tweaker

## Evolution
- ⬜ X-Session

## FaderFox
- ⬜ DJ2

## Gemini
- ⬜ CDMP-7000 · ⬜ FirstMix

## Hercules
- ⬜ DJ Console 4-Mx · ⬜ DJ Console (Mac Edition) · ⬜ DJ Console MK1 · ⬜ DJ Console MK2
- ⬜ DJ Console MK4 · ⬜ DJ Console RMX · ⬜ DJ Console RMX2 · ⬜ DJControl MIX · ⬜ DJControl AIR
- ⬜ DJControl Compact · ⬜ DJControl Inpulse 200 · ⬜ DJControl Inpulse 300 · ⬜ DJControl Inpulse 500
- ⬜ DJControl Instinct (S) · ⬜ DJControl Jogvision · ⬜ DJControl MP3 · ⬜ DJControl MP3 e2 / LE / Glow
- ⬜ DJControl Starlight · ⬜ P32 DJ

## Icon
- ⬜ iControls · ⬜ P1-Nano

## Intech Studio
- ⬜ Grid (TEK2)

## ION
- ⬜ Discover DJ · ⬜ Discover DJ Pro

## Keith McMillen
- ⬜ QuNeo

## Kontrol DJ
- ⬜ KDJ500

## Korg
- ⬜ Kaoss DJ · ⬜ nanoKONTROL · ✅ **nanoKONTROL2** (bundled starter profile; sliders→deck gains,
  knobs→filters, S→cue, M→sync, + crossfader/master — from the CC-mode default map) · ⬜ nanoPAD2

## M-Audio
- ⬜ X-Session Pro · ⬜ Torq Xponent

## Miditech
- ⬜ Midicontrol

## Mixman
- ⬜ DM2

## MixVibes
- ⬜ U-Mix Control (Pro) 2

## M-Vave
- ⬜ SMC-Mixer · ⬜ SMK-25 II

## Native Instruments (many are **HID**)
- ⬜ Traktor Kontrol F1 · ⬜ Kontrol S2 MK1 · ⬜ Kontrol S2 MK2 · ⬜ Kontrol S2 MK3 · ⬜ Kontrol S3
- ⬜ Kontrol S4 MK2 · ⬜ Kontrol S4 MK3 · ⬜ Kontrol X1 · ⬜ Kontrol Z1

## Nintendo
- ⬜ Wiimote (HID / motion)

## Novation
- ⬜ Dicer · ⬜ Launchpad Mini · ⬜ Launchpad MK1 · ⬜ Launchpad MK2 · ⬜ Twitch

## Numark
- ⬜ DJ2GO · ⬜ DJ2GO2 Touch · ⬜ iDJ Live II · ⬜ Mixtrack · ⬜ Mixtrack Platinum · ⬜ Mixtrack Platinum FX
- ⬜ Mixtrack Pro · ⬜ Mixtrack (Pro) 3 · ⬜ Mixtrack Pro FX · ⬜ Mixtrack Pro II · ⬜ N4 · ⬜ NS6II
- ⬜ NS7 · ⬜ Omni Control · ⬜ Party Mix · ⬜ Scratch · ⬜ Total Control · ⬜ V7

## Pioneer
- ⬜ CDJ-2000 · ⬜ CDJ-350 · ⬜ CDJ-850 · ⬜ DDJ-200 · ⬜ DDJ-400 · ⬜ DDJ-FLX4 · ⬜ DDJ-SB · ⬜ DDJ-SB2
- ⬜ DDJ-SB3 · ⬜ DDJ-SX

## Reloop
- ⬜ Beatmix 2 · ⬜ Beatmix 4 · ⬜ Beatpad · ⬜ Digital Jockey 2 Controller Ed. · ⬜ Digital Jockey 2 Interface Ed.
- ⬜ Digital Jockey 2 Master Ed. · ⬜ Jockey 3 Master Ed. · ⬜ Mixage · ⬜ Terminal Mix 2/4

## Roland
- ⬜ DJ-505

## Sony
- ⬜ Sixaxis (HID)

## Soundless Studio
- ⬜ joyMIDI

## Stanton
- ⬜ DJC.4 · ⬜ SCS.1d · ⬜ SCS.1m · ⬜ SCS.3d "DaScratch" · ⬜ SCS.3m "DaMix"

## Tascam
- ⬜ US-428

## TrakProDJ
- ⬜ TrakProDJ

## Vestax
- ⬜ Spin · ⬜ Typhoon · ⬜ VCI-1000 · ⬜ VCI-100 MKI · ⬜ VCI-100 MKII · ⬜ VCI-300 · ⬜ VCI-400

## Yaeltex
- ⬜ MiniMixxx

---

### Implementation priority (suggested)

Start with widely-owned, **class-compliant MIDI** units (no HID needed) for fast wins, then expand:

1. Pioneer DDJ-FLX4 / DDJ-400 / DDJ-SB3 · Numark Mixtrack series / Party Mix · Hercules Inpulse 200/300/500
2. Korg nanoKONTROL2 · Akai LPD8 · Novation Launchpad (pad grids) · Allen & Heath Xone:K2
3. Denon MC-series · Reloop Beatmix/Terminal Mix · Roland DJ-505
4. **HID** wave (needs the `hidapi` input layer): Native Instruments Traktor Kontrol S2/S4/X1/Z1, etc.

Each lands as a profile PR with its manual + product-page links and a clean-room note.
