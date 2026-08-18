# Measured string-body resonances → modal-body fit

Reference: **University of Iowa Musical Instrument Samples** (MIS), arco *mf*
chromatic runs (violin, viola, cello, double bass). Public research samples;
audio **not** committed (gitignored `data/`). Only the extracted body-resonance
measurements inform the model.

## Method

Body resonances are the **fixed** spectral peaks that stay boosted regardless of
which note plays. A naive long-term-average spectrum is confounded by the played
fundamentals (peaks land on the open-string pitches). Fix: **harmonic pooling** —
per frame detect f0, sample each harmonic h·f0 (Goertzel), source-compensate
(×h), and accumulate into an absolute-frequency histogram. Different notes
illuminate each physical frequency with different harmonics, so the note
fundamentals wash out and the fixed body transfer function emerges (cross-checked
against a whitened LTAS).

## Violin (the template)

Peaks: a strong cluster at **400–460 Hz** (A1/B1± air-and-wood), a wood peak near
**820 Hz**, and **~1750 Hz** (bridge-hill region) — corroborating the literature
A0/B1±/bridge-hill picture. The model's `TPL` body table encodes these.

## Viola

The violin template maps onto the viola **almost perfectly under a single
frequency scale**: main wood 453/530 = 0.855; upper trio 881/1142/1440 vs
1100/1400/1750 = 0.80–0.82. → `body_scale(Viola) = 0.82`. No separate table.

## Double bass — a different shape, its own table

Measured body transfer (peak-normalized):

| Hz | rel | |
|----|-----|--|
| 57 | 0.81 | A0 main air |
| 85 | 0.90 | wood |
| **101** | **1.00** | **main wood (strongest)** |
| 127 | 0.91 | wood |
| 147 | 0.65 | |
| 170 | 0.56 | |
| 196 | 0.38 | |
| >270 | ≤0.20 | steep roll-off, negligible bridge hill |

The bass is **not** a scaled violin: energy is a tight **57–170 Hz** cluster with
a steep roll-off above ~200 Hz and no bridge hill. The old `body_scale = 0.36`
was ~2× too high — it placed the template's lowest mode at 101 Hz (the bass's
*main wood*) and left the real ~57 Hz air resonance unmodeled. So the bass now
gets its own explicit `TPL_BASS` mode table (see `Body::new`), branched on
`StringKind::Bass`.

## Applied to `bowed-core`

- Violin: `TPL` (unchanged, corroborated).
- Viola: `body_scale` 0.80 → **0.82**.
- Cello: 0.52 (not re-measured this pass; a follow-up candidate — likely also a
  touch high given the bass result).
- Bass: dedicated **`TPL_BASS`** table (air 57, main wood 101, steep HF roll-off).
