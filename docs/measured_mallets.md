# Measured mallet-percussion modes → model fit

Reference: **University of Iowa Musical Instrument Samples** (MIS), *ff*, single
notes (marimba yarn, xylophone hard-rubber, vibraphone sustain, orchestral
bells). Public research samples; audio **not** committed (kept under the
analyzer's gitignored `data/`). Only the extracted measurements inform the model.

Method: 3 notes per instrument (low/mid/high). Partials from a **150 ms**
Hann-windowed FFT at the attack (upper inharmonic partials decay too fast to
survive a 1 s window). f0 by autocorrelation refined to the nearest spectral
peak. T60 per partial by tracking a Goertzel magnitude over time and fitting the
−60 dB slope. Note: MIS notes are the real acoustic ring (resonators + hall,
undamped), so measured decays run longer than a musical synth model usually wants.

## Measured partial ratios (relative to the fundamental)

| instrument | 1 | 2nd | 3rd | 4th | note |
|------------|---|-----|-----|-----|------|
| Marimba    | 1.00 | **4.00** | ~9.9 | — | tuned 1 : 4 : 10 (was modelled 1 : 3.9 : 9.2) |
| Xylophone  | 1.00 | **3.00** | ~6.5 | (9.5 weak) | 3:1 exact; 3rd was 6.0, measured ~6.5 |
| Vibraphone | 1.00 | **4.00** | 10.0 | — | tuned dead-on 1 : 4 : 10 |
| Orch. bells| 1.00 | 3.0–3.3 | ~6.0 | ~11.4 | see note below |

The marimba and vibraphone both measure **1 : 4.00 : 10.0** tightly across all
notes — the model's old 1 : 3.9 : 9.2 was ~3 % / ~8 % low. Corrected.

## Measured decay (T60)

- Marimba: fundamental ~1.4 s (C4) → 4.6 s (C2); the 10× partial rings only
  ~0.08× as long. Pitch-decay exponent **p ≈ 0.85**.
- Xylophone: ~0.4–1.3 s, strongly front-loaded (perceptually dry); **p ≈ 1.0**.
- Vibraphone (pedal up): fundamental **15–24 s**, the 10× partial dies in ~1 s.
  **p ≈ 0.35** (shallow — high and low bars ring similarly long).

## Applied to `mallet-core`

- Marimba: partials → **4.0, 9.9**; 3rd-partial decay → 0.15; `decay_pitch` 0.7 → **0.85**.
- Xylophone: 3rd partial → **6.5**; partial decays → 0.30 / 0.12; `decay_pitch` 0.5 → **1.0**.
- Vibraphone: partials → **4.0, 10.0**; 3rd-partial decay → 0.10;
  `base_t60` 4.0 → **9.0**; `decay_pitch` 0.6 → **0.35**.
- Glockenspiel: `base_t60` 0.9 → **1.3** (metal bars ring longer than 0.9 s).

## A modelling choice left deliberate: glockenspiel

The Iowa "bells" set is **orchestral bells**, whose bars measure ~1 : 3.1 : 6.0 :
11.4 — noticeably higher than the *ideal free-bar* ratios 1 : 2.756 : 5.404 :
8.933 the model uses. The free-bar numbers are physically correct for a thin
steel glockenspiel bar (a different, brighter object than Iowa's orchestral
bells), so the ratios were **kept**; only the too-short decay was lengthened.
Switching to the orchestral-bells ratios would be a separate instrument voice.
