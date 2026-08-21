<!--
  Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.
-->
# Puget.audio — physical-modeling fidelity roadmap

Where we push realism next. The instrument library (handpan, five orchestral
winds + ney/sorna + three Irish winds, bowed strings incl. kamancheh/fiddle,
mallets, plucked lutes/zithers, membrane drums) and the platform layers (the
universal macro/CV engine, the Scala `.scl` microtonal quantizer, the dastgah
tuning pack) are **built**. This roadmap is specifically about **model
fidelity** — closing the gap between "clearly synthetic" and "convincingly
real", and the DSP techniques that get us there.

Status: ✅ shipped · 🔬 spike (prototype + measure) · 🛠️ build · 🧭 research

## Recently shipped (the near-term realism pass)

- ✅ Realism audit + fixes: sax breath dead-zone, bow-force cliff, mallet
  velocity→brightness, plucked tuning/inharmonicity/shimmer/sustain.
- ✅ Wind **de-rasp**: band-limited, **flow-correlated** turbulence (noise pulses
  with the aperture flow instead of hissing over the tone).
- ✅ Wind **pitch-flutter**: a few-cent zero-mean bore-length wander so the tone
  breathes instead of sitting on a dead-steady synthetic pitch.
- ✅ Bowed **ring-out**: a bow-contact model so a lifted bow leaves the string to
  ring freely.
- ✅ Uilleann **drone**: hollow odd-harmonic reed-pipe tone (was a harsh saw).

## The ceiling we're at

Every wind runs a **digital waveguide** (delay-line bore) driven by a **static,
memoryless exciter** — the reed is a lookup curve, the jet a cubic. Efficient,
stable, firmware-friendly — but the static exciter is the shared root cause of
the three things still holding us back:

- the **clarinet under-generates upper harmonics** (too-linear an operating point);
- the **sorna destabilized** when steepened (a static curve can't self-limit);
- the reeds read **"fake"** — no reed resonance, no inertia, no beating.

## The fidelity ladder

### Tier 1 — Dynamic exciters (the keystone) 🔬

The single highest-leverage change. Replace the static curves with exciters that
have their own *dynamics*.

| Item | Technique | Fixes | Effort | Risk | Firmware |
|---|---|---|---|---|---|
| **Dynamic reed** | reed = damped mass-spring resonator + Bernoulli flow (`u = w·y·√\|Δp\|`) + beating/collision, replacing the lookup table | sorna (stable *and* buzzy), reed "fakeness", clarinet richness — **subsumes Pass C for winds** | med | high (self-oscillator) | yes |
| **Improved jet** | jet delay + jet dynamics (McIntyre–Schumacher–Woodhouse), not a memoryless cubic | ney octave-bistability, flute breath realism | low–med | med | yes |

### Tier 2 — Accurate bores 🔬🧭

| Item | Technique | Fixes | Effort | Risk | Firmware |
|---|---|---|---|---|---|
| **Conical bore** | true conical waveguide sections / spherical-wave junctions, replacing cylinder + faked evens | correct sax & sorna spectrum + octave register | med | med | yes |
| **Tone-hole lattice** | each open/closed hole a two-port scattering junction → the woodwind cutoff frequency | timbre & register *vs pitch*, the "which note sounds how" behavior | med–high | med | tight |

### Tier 3 — Strings 🛠️

| Item | Technique | Fixes | Effort | Risk | Firmware |
|---|---|---|---|---|---|
| **Bow-friction richness** (Pass C, strings) | reshape the stick-slip friction / exciter so firm bowing generates rich upper harmonics + bridge-hill, not a dull tone | bowed dullness under load (audit finding) | med–high | high (self-oscillator) | yes |

### Tier 4 — Space & polish 🛠️

| Item | Technique | Fixes | Effort | Risk | Firmware |
|---|---|---|---|---|---|
| **Shared room reverb** | one tunable no_std stereo room/plate stage (trimmed Freeverb / FDN) any voice can sit in | every non-handpan voice sounds "dry" | low–med | low (additive) | yes |
| **Loss + radiation filters** | visco-thermal boundary-layer loss + proper radiation impedance, replacing one-pole approximations | incremental warmth/air across all winds | low | low | yes |
| **Handpan attack + gu** | pitched high-mode "tak" ping; coupled Helmholtz+shell gu cavity | handpan attack/bass polish | low | none | yes |

### Tier 5 — Reference-grade (research) 🧭

| Item | Technique | Fixes | Effort | Risk | Firmware |
|---|---|---|---|---|---|
| **Modal bore / FDTD "HD" mode** | finite-difference (Bilbao) or modal input-impedance bore + coupled exciter | reference-grade everything | high | — | **desktop/VST only** |

## Execution plan

1. **De-risk in parallel (this kickoff).** Spike Tier 1–4 items in isolated
   worktrees: build a working proof-of-concept, A/B it against the current model
   with measurements, and report **clear-win / marginal / no-improvement /
   infeasible** + a recommendation. No fragile-oscillator change ships un-proven.
2. **Synthesize** the spikes into a ranked integration order (dependencies,
   risk, effort).
3. **Integrate winners one voice at a time**, committing to the branch, A/B
   demos each step — starting with whichever Tier 1 exciter proves out, since it
   unblocks the most (sorna, reed realism, wind-side Pass C).
4. Ship the low-risk isolated deliverables (room reverb, handpan polish)
   independently as they land.

The keystone is the **dynamic reed**: prove it on the clarinet (best reference)
and the sorna (worst offender) first; if it beats the current model, it becomes
the new wind exciter across the family.
