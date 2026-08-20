<!-- Design doc for the universal Macro Engine + Expander Bus (crate puget-macro).
     Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved. -->

# The universal Macro Engine + Expander Bus

`puget-macro` is the keystone layer that maps a small, fixed set of **universal
macros** onto every instrument family (handpan, wind, bowed, mallet, plucked). It
is deliberately built in a **CV-first, modular idiom** — knobs, attenuverters,
normalled CV, slew, gates, and an instrument-agnostic message bus — not
host-automation floats. One universal expander (or one poly-CV cable) can drive
any family base through the same seven macros.

The engine is `no_std` + optional `libm`, dependency-free (internal workspace
crates only), and every family adapter is feature-gated so a Daisy build for one
module compiles only its own family's mapping.

## The seven universal macros

Every macro's canonical value is a normalized `f32` in `0.0..=1.0`. The engine
scales that into each family's native units.

| Macro | Meaning (the contract every adapter honors) |
|---|---|
| **Timbre** | Spectral colour: dark ↔ bright / metallic. |
| **Dynamics** | Playing intensity: breath/bow drive for sustained voices; ring-openness (sustain / un-damping) for struck ones. |
| **Motion** | Modulation liveliness: vibrato, tremolo, breath sway, didge wobble, nonghyeon shake. |
| **Articulation** | Contact / attack character: bow position, growl, amp grind, sympathetic halo, pitch push. |
| **Chairs** | Ensemble size: soloist ↔ full section. |
| **Spread** | Detune spread across the section (cents). |
| **Width** | Stereo image: mono ↔ full field. |

Where a macro doesn't apply to a family (e.g. `Chairs` on a single, non-sectioned
voice, or `Timbre` on a struck mallet whose colour is fixed by its mode data) the
adapter reports it **unsupported** and `apply_bus` skips it — a documented no-op,
never a silent wrong mapping.

## Per-family mapping table (macro × family → concrete parameter)

Each cell is the family's existing public setter, with the `0..1` macro scaled to
the noted range. `v` is the normalized macro value. `—` = unsupported no-op.

| Macro | Handpan (voice / choir) | Wind (voice / section) | Bowed (voice / section) | Mallet (voice / section) | Cifteli | Gayageum | Basitar |
|---|---|---|---|---|---|---|---|
| **Timbre** | `set_shell(v·0.4)` | `set_brightness(v)` | `set_brightness(v)` | — (fixed by modes) | `set_brightness(v)` | `set_brightness(v)` | `set_brightness(v)` |
| **Dynamics** | `set_damp(1−v)` | `set_breath(v)` | voice: `set_bow(v,v)` · section: — | `set_damp(1−v)` | `set_sustain(v)` | `set_sustain(v)` | `set_sustain(v)` |
| **Motion** | `set_air(v)` | `set_vibrato(5.5, v·0.4)` | `set_vibrato(5.5, v·0.05)` | `set_tremolo(5.5, v)` | — | `set_vibrato(5.5, v·80c)` | — |
| **Articulation** | `set_coupling(v·0.5)` | `set_growl(v·0.3, 5.5)` | `set_bow_position(v)` | — | — | `set_bend(v·200c)` | `set_drive(v)` |
| **Chairs** | choir: `set_chairs` · voice: — | section: `set_chairs` · voice: — | section: `set_desks` · voice: — | section: `set_chairs` · voice: — | — | — | — |
| **Spread** | choir: `set_spread(v·12c)` · voice: — | section: `set_spread(v·12c)` · voice: — | section: `set_spread(v·12c)` · voice: — | section: `set_spread(v·12c)` · voice: — | — | — | — |
| **Width** | choir: `set_width(v)` · voice: — | section: `set_width(v)` · voice: — | section: `set_width(v)` · voice: — | section: `set_width(v)` · voice: — | — | `set_width(v)` | `set_width(v)` |

Notes on the less-obvious choices:

- **Handpan is struck**, so it has no continuous exciter. Its macros land on the
  resonant-body controls: shell nonlinearity (its metallic combination-tone
  colour) is Timbre, palm-mute is inverted into Dynamics (more dynamics = more
  open ring), room air is Motion (spatial movement), sympathetic coupling is
  Articulation (how a struck field haloes its neighbours).
- **Bowed section Dynamics is unsupported**: a section's bow energy is a per-note
  property set at note-on (the trigger event), not a live setter, so continuous
  Dynamics has nothing to drive there. On the single `Bowed` voice, Dynamics maps
  to `set_bow(v, v)` — harder playing is a faster *and* heavier bow.
- **Mallet Timbre/Articulation are unsupported**: a marimba is a marimba; its
  spectral character is the chosen instrument's mode set, not a knob.
- The three **plucked voices** share Timbre = brightness and Dynamics = sustain,
  then diverge on character (gayageum nonghyeon vibrato/bend, basitar amp grind).

The scaling constants (`SPREAD_MAX_CENTS = 12`, `CHAIRS_MAX = 12`,
`MOTION_RATE_HZ = 5.5`, per-family depth caps) live in one place
(`src/target.rs` and the adapter headers) so the whole mapping's feel is tunable
without hunting.

### One minimal core edit

The only change to a core crate was adding `HandpanEnsemble::set_shell(amount)` —
a one-line fan-out that calls each chair's existing `Handpan::set_shell_nonlin`,
so the choir's Timbre maps to the same parameter the single voice does. Every
other mapping uses setters that already existed.

## CV conventions (volts, attenuversion, normalling, slew)

All in `src/cv.rs`, matching the platform's existing conventions (the `cxx`
bridge notes "VCV audio ±5 V, V/Oct 1 V/oct, 0 V = the ding").

### Reference voltages

| Signal | Reference | Maps to |
|---|---|---|
| Audio | ±5 V (`AUDIO_PEAK_V`) | voice output ~±1, scaled ×5 on the way out |
| Unipolar modulation | 0–10 V (`CV_UNIPOLAR_V`) | macro value `0..1` |
| Bipolar modulation / LFO | ±5 V (`CV_BIPOLAR_V`) | attenuverter CV `-1..1` |
| Gate / trigger | Schmitt `GATE_LOW_V = 0.4` / `GATE_HIGH_V = 1.0` | high/low + edge |
| Pitch | 1 V/oct, 0 V = root | `hz_from_volts` / `volts_from_hz` |
| Velocity / accent | 0–10 V | `velocity_from_volts` → `0..1` |

V/Oct → Hz and microtonal quantization **reuse `puget_dsp::tuning`** (the
`Quantizer`'s `quantize_hz` / `quantize_volts`) rather than re-deriving the
one-volt-per-octave law; `cv.rs` only adds the thin `hz_from_volts` /
`volts_from_hz` helpers and the round-trip is tested against the shared quantizer.

### Knob + attenuverter + normalled CV

`CvInput` is the standard per-macro control conditioner:

- **Unpatched** — the CV jack is normalled to the knob, so the output *is* the
  knob (the attenuverter has nothing to act on).
- **Patched** — output is `clamp01(knob + attenuverter · CV)`, where CV is the
  jack conditioned to a normalized signal (unipolar `0..1`, or bipolar `-1..1`).

*Design fork, resolved toward the modular idiom:* the attenuverter multiplies the
**patched CV** and the knob is the resting value (CV modulates *around* the knob),
rather than the CV *replacing* the knob. That keeps an unpatched panel dead-simple
and reads like a Maths/attenuverter row. It is intentionally distinct from the
cores' *signal-level* normalling (e.g. `Wind::set_breath_mod`, where an unpatched
jack falls back to an onboard LFO) — that behaviour is surfaced through the
**Motion** macro instead.

### Slew

`Slew` is a one-pole (exponential) limiter: a step toward a new target settles
**monotonically** (no overshoot), so macro knob/CV jumps glide instead of
zippering. Set by a glide time in ms at the sample rate; `time_ms = 0` is
instantaneous. Cheap enough for per-sample use, and equally fine per control
block. The `MacroEngine` runs one `Slew` per macro.

### Gate / velocity

`Gate` is a Schmitt trigger with hysteresis between `GATE_LOW_V` and
`GATE_HIGH_V`; `process(volts)` returns `Rising` / `Falling` / `None` and holds a
high/low state — the "did a trigger arrive this sample" input for strikes,
note-ons and clocks. `velocity_from_volts` conditions a unipolar velocity CV.

## The expander-bus protocol

`MacroBus` (in `src/bus.rs`) is the instrument-agnostic message the way a VCV Rack
expander passes a fixed struct across the base↔expander boundary:

- **Fixed-size, `Copy`, alloc-free.** Seven `f32` values (one per macro) plus a
  per-macro **present** bit (a `u8` mask). No heap, no per-sample allocation —
  drops straight onto firmware and into a VCV expander's `producerMessage`.
- **`{macro_id, value}` messages.** `MacroMessage { id, value }` is the atomic
  update; `MacroBus::apply(msg)` (or `set(id, v)`) writes one macro and marks it
  present. A poly-CV cable maps naturally: channel *i* ↔ `MacroId::from_index(i)`.
- **Present bits = partial buses.** An expander that only drives Timbre and Width
  sets just those two; `MacroTarget::apply_bus` applies only macros that are
  **both present on the bus and supported by the target**, leaving everything else
  to the base's own panel.

`MacroEngine` is the base's own front panel: one `CvInput` + one `Slew` per macro.
Feed it the conditioned CV for each macro (`Some(unipolar)` patched, `None` open)
and it returns a slewed `MacroBus`. Same struct whether it comes from the base's
knobs or from an expander — the target can't tell the difference.

### Data flow

```
knob + attenuverter + CV jack ─▶ CvInput.eval ─▶ Slew ─▶ MacroBus[id]
   (or an expander's own panel) ─────────────────────────▶ MacroBus[id]
                                                              │
                                        MacroTarget::apply_bus│ (present & supported)
                                                              ▼
                                     family setter (set_brightness, set_width, …)
```

## Public API surface

- `MacroId` (+ `ALL`, `index`, `from_index`, `name`), `MACRO_COUNT`.
- `cv`: `CvInput`, `Slew`, `Gate`/`Edge`; `hz_from_volts`/`volts_from_hz`,
  `unipolar_from_volts`/`volts_from_unipolar`, `bipolar_from_volts`/
  `volts_from_bipolar`, `velocity_from_volts`; the voltage-reference constants.
- `MacroBus`, `MacroMessage`, `MacroEngine`.
- `MacroTarget` (`apply_macro`, `supports`, default `apply_bus`), implemented for
  `HandpanInstrument`, `HandpanEnsemble`, `Wind`, `WindEnsemble`, `Bowed`,
  `BowedEnsemble`, `Mallet`, `MalletEnsemble`, `Cifteli`, `Gayageum`, `Basitar`
  (each behind its family feature).

## Build / test

```text
cargo test -p puget-macro                                   # host, all families
cargo build -p puget-macro --no-default-features --features libm,handpan   # no_std, one family
cargo test --workspace
```

The `libm,<family>` build proves each adapter compiles standalone for firmware
with only its own core pulled in.
