# Expression across the plugin and the modules

Every expressive feature is built once in `handpan-core`, then surfaced two
ways: as MIDI/MPE/automation in the VSTi, and as CV/gate/knobs in the Eurorack
modules. The **brain** module generates *intent* (note, velocity, articulation,
gu, timing); the **voice** module is the instrument that consumes it. This keeps
the Unix split: either module is useful alone, and together they play
expressively from one clock.

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

## Panel tiers (voice module)

**Compact (~8 HP)** — one expression axis, cheap, playable.
- In: V/Oct, Strike, Velocity, **Feel** (one macro CV morphing artic+position+damp)
- Out: L, R
- Knobs: Scale, Size/Build, Air, Feel

**Expressive (~12 HP)** — full articulation control, more "played".
- In: V/Oct, Strike, Velocity, **Damp**, **Artic**, **Position**, **Gu** (gate)
- Out: L, R
- Knobs: Scale, Size/Build, Air, Tune

## Brain module (~6 HP)

- In: Clock, knobs (Scale, Mode, Density, Feel)
- Out: Gate, 1V/oct CV, **Aux CV** (generated articulation / accent)

## Build order

1. `handpan-core` DSP first (shared, lights up both products):
   articulations (open/mute/slap), gu strike, strike-position timbre,
   continuous `set_damp`, `set_tune`.
2. VSTi: map MIDI/MPE/CC → the new APIs; preset system + GUI.
3. Firmware: map CV/gate/knobs → the new APIs; pick a panel tier.

The core work is the same regardless of which product ships first, so it is the
right next step.
