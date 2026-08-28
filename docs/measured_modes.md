# Measured modal data

Extracted by `handpan-analyzer` from single-note handpan recordings, aggregated
across the 9 D-Kurd notes (D3, A3, B♭3, C4, D4, E4, F4, G4, A4).

Recordings: MUMT307 handpan project, Jiali Cheng, McGill University
(<https://github.com/carrieeex/MUMT307-project>, `sounds-original/`). Used for
offline parameter extraction only; the audio is not redistributed here.

## Aggregate timbre (median across notes)

| partial | ratio | rel. gain | decay ×fund | n |
|--------:|------:|----------:|------------:|--:|
| fundamental | 1.000 | 1.00 | 1.00 | 13 |
| octave      | 2.004 | 0.41 | **1.78** | 11 |
| comp. fifth | 3.002 | 0.26 | **2.04** | 12 |
| double 8ve  | 3.996 | 0.043 | 1.38 | 6 |

## Key findings

1. **Tuned partials are 1 : 2 : 3 (: 4) to within a few cents** — confirms the
   maker target and matches the acoustics literature.
2. **The octave and compound fifth decay ~1.8–2.0× longer than the
   fundamental.** The fundamental fades first while the octave/fifth sustain —
   the "singing" quality of a good handpan. (The model previously had upper
   partials decaying *faster*; corrected in `HANDPAN_TIMBRE`.)
3. **Mode splitting is real**: several notes show the octave or fifth as two
   partials a few cents apart (e.g. D4 fifth at 2.951 and 2.999), which is the
   source of the slow beating/shimmer — validates the detuned-doublet timbre.
4. Weak inharmonic partials appear around 1.1–1.6× and ~5.2× the fundamental
   (interference / shell modes), contributing air but little sustained energy.

Regenerate with: `cargo run -p handpan-analyzer -- data/mcgill`
