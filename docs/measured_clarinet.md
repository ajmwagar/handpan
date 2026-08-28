# Measured clarinet spectra → model fit

Reference: **University of Iowa Musical Instrument Samples** (MIS), Bb clarinet,
*ff*, anechoic single notes (2014 pitch set). Public research samples; the audio
is **not** committed (kept under the analyzer's gitignored `data/`). Only the
extracted measurements below — facts about the instrument — inform the model.

Harmonic levels are dB relative to the strongest harmonic; "crest" is
peak/RMS of a steady-state window (a sine ≈ 1.41, a square ≈ 1.0).

## What the recordings show

| note | f0 (Hz) | crest | h1 | h2 | h3 | h4 | h5 | h6 | h7 | h9 |
|------|---------|-------|----|----|----|----|----|----|----|----|
| D3   | 146     | 2.35  | 0  | −29| −3 | −17| −4 | −14| −10| −14|
| G3   | 196     | 1.97  | 0  | −37| −1 | −30| −10| −27| −26| −30|
| D4   | 292     | 1.98  | −1 | −33| 0  | −22| −14| −21| −26| −36|
| A4   | 441     | 2.43  | 0  | −13| −18| −42| −31| −37| −52| −46|
| D5   | 588     | 1.81  | 0  | −17| −10| −27| −28| −37| −51| −63|

Three facts drive the model:

1. **Odd-harmonic dominant, but not purely.** Evens sit ~15–35 dB down (bell and
   tone-hole radiation, not the ideal cylinder) — present, never absent.
2. **A tone-hole-lattice cutoff near ~1.4 kHz.** Harmonics *below* it stand
   strongly and fairly flat (D3 keeps h3…h9 within ~14 dB); *above* it they roll
   off steeply. It is a roughly **fixed frequency**, so low notes are rich and
   the altissimo is comparatively pure — exactly the register trend in the table.
3. **Peaky waveform** (crest ~1.8–2.4), i.e. a hard-blown reed making sharp flow
   pulses — far from the near-sinusoid a lightly-driven waveguide produces.

## How the model is fit

The raw waveguide (cylindrical bore + STK reed table + one-pole loss filter set
to the ~1.4 kHz cutoff) reproduces the odd-dominance and the register trend, but
on its own sits at a purer, weaker-harmonic operating point than a real *ff*
clarinet (the simple reed table either stays quasi-linear or chokes shut if
driven harder). To reach the measured balance without destabilising the
oscillator, a **radiation/formant EQ fit to the table above** shapes the output:

- a broad presence lift (~1 kHz, +13 dB, low-Q) that re-flattens the strong
  sub-cutoff harmonics — restoring h3 from ~−16 dB back toward the measured ~0,
- a smaller lift near the ~3 kHz clarinet formant,
- a small DC-tracked squared term seating the even harmonics around −30…−40 dB.

Result (model vs. real): D3 crest 2.25 vs 2.35, D4 1.88 vs 1.98; the low/mid
register — where the clarinet mostly lives — tracks the reference closely.

Re-measure any note against the model with:

```
cargo run -q -p wind-core --example probe -- <midi> <brightness>
```
