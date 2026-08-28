# Expression across the plugin and the modules

Every expressive feature is built once in `handpan-core`, then surfaced two
ways: as MIDI/MPE/automation in the VSTi, and as CV/gate/knobs in the Eurorack
hardware.

## Architecture (refined): one playable module

The voice is a *detailed, playable handpan*, so the product is **one
self-contained module**, not a voice+brain pair:

- **Quantizer built in.** The V/Oct input snaps to the current scale's tone
  fields (`strike_voct`), so it plays from anything — keyboard, LFO, joystick,
  a sloppy CV source — and lands on handpan notes. No external quantizer needed.
- **Expression comes from gesture, and the ecosystem already excels at it.** A
  joystick maps naturally: X = field (pitch), Y = strike position/timbre,
  Z/pressure = palm-mute damp, button = strike. 0-CTRL, Tetrapad, Pressure
  Points, any sequencer all drive it. We don't ship our own controller.
- **It still plays itself.** The internal generative (Wander / Euclid) is
  *normalled* to the Strike input: nothing patched → it plays itself; patch a
  controller → that takes over.
- A dedicated generative **brain** stays possible as an *optional later
  companion*, but is not required for v1.

## Feature → surfaces

| Feature | Core API (to add) | VSTi | Voice module input | Brain can generate |
|---|---|---|---|---|
| Articulation: open / mute / slap | `strike(field, vel, Artic)` | key-range or CC | Artic CV (sampled at Strike) | random / patterned artic |
| Gu (bottom-port bass hit) | `strike_gu(vel)` | low key / MIDI ch | dedicated Gu gate | separate gu rhythm |
| Strike position (center↔edge) | `position` at strike | MPE Y / CC74 | Timbre CV (or share Artic) | yes |
| Pressure / palm-mute | `set_damp(amount)` continuous | MPE pressure / aftertouch | Damp CV (continuous) | LFO / envelope |
| Pitch (+ bend) | `strike_voct` (exists) | pitch bend | V/Oct (exists) | quantized V/oct out |
| Velocity / accent | (exists) | velocity | Velocity CV (exists) | accent pattern CV |
| Tune / reference (432, fine) | `set_tune(cents)` | global param / Scala | Tune knob | — |
| Scale / Size / Build / Air / Shell | (exists) | knobs + presets | panel knobs | Scale select |
| Presets | `VoiceProfile` snapshots | preset browser | knobs are the preset (+ optional slots) | pattern / preset recall |

Notes:
- **Microtuning**: full Scala/temperament import is a plugin-only feature; the
  module gets a global fine-tune knob plus a small set of temperament options
  (equal / just / 432) — no file browser on hardware.
- **Presets in hardware** are the panel state. Optional snapshot slots (recall
  by button/CV) only make sense if the panel gains an encoder + small display.

## Panel tiers (the one module)

`V/Oct` is scale-quantized (the built-in quantizer); `Strike` has the internal
generative normalled to it (unplugged = plays itself).

**Compact (~8–10 HP)** — one expression axis, cheap, playable.
- In: V/Oct (quantized), Strike (norm→generative), Velocity, **Feel** (one macro
  CV morphing artic+position+damp)
- Out: L, R
- Knobs: Scale, Size/Build, Air, Feel

**Expressive (~12–14 HP)** — full articulation control, more "played".
- In: V/Oct (quantized), Strike (norm→generative), Velocity, **Damp**,
  **Artic**, **Position**, **Gu** (gate)
- Out: L, R
- Knobs: Scale, Size/Build, Air, Tune

Optional out: a **Pitch CV** thru (the quantized field's V/oct) so the module
can double as a handpan quantizer for the rest of the rack.

## Optional brain companion (later, not v1)

If a dedicated generative module is ever wanted: Clock + knobs (Scale, Mode,
Density, Feel) → Gate, 1V/oct CV, **Aux CV** (generated articulation / accent).
The internal normalled generative covers the "plays itself" case without it.

## Build order

1. `handpan-core` DSP — **done**: articulations (open/mute/slap), gu strike,
   strike-position timbre, continuous `set_damp`, `tune_cents`.
2. VSTi: map MIDI/MPE/CC → the APIs; preset system + GUI.
3. Firmware: one module — quantized V/Oct + Strike (norm→generative) + Velocity
   + expression CVs → the APIs; stereo out. Pick a panel tier.

The core is shared, so either front-end can ship first.
