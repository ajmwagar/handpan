<!--
  Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.
-->
# Dastgah tuning pack

Persian classical music is organised around the **dastgah** system — a set of
modal frameworks, each with its own scale, hierarchy of important notes, and
repertoire of melodic figures (*gusheh*). What makes the scales impossible to
play on a 12-note keyboard is the use of **neutral intervals**: pitches roughly
a quarter-tone away from their Western neighbours, notated with two accidentals:

- **koron** (♭ with a slash) — lowers a note by ~**60 cents** (a bit more than a
  quarter-tone flat);
- **sori** (♯ with a slash) — raises a note by ~**40 cents**.

These offsets are *approximate and flexible*. Persian intonation is not a fixed
equal temperament: it varies by region, by master, by instrument, and even with
the melodic direction. Hormoz Farhat described the intervals as deliberately
"flexible." The cents below are one internally-consistent measured set (Abdoli),
which we use as sensible, playable defaults — not as the one true tuning.

Because these live as standard **Scala `.scl`** files, the `puget_dsp::tuning`
quantizer loads them directly, so any voice fed a plain V/Oct sequence snaps to
the mode. Retune the root (the *shahed* / tonic) to move the dastgah to any key.

## The scales

Degrees in cents from the tonic (`0`), octave `1200`. Files in
[`scales/dastgah/`](../scales/dastgah).

| Dastgah | 2nd | 3rd | 4th | 5th | 6th | 7th | Character |
|---|---|---|---|---|---|---|---|
| **Shur** | 149 | 300 | 500 | 702 | 783 | 985 | central, introspective |
| **Segah** | 198 | 352 | 495 | 707 | 826 | 1013 | warm; tonic sits on a koron |
| **Chahargah** | 134 | 397 | 497 | 634 | 888 | 994 | dramatic, wide steps |
| **Homayoun** | 100 | 398 | 502 | 715 | 800 | 990 | majestic, wistful |
| **Mahur** | 208 | 397 | 497 | 702 | 891 | 994 | bright, major-like |
| **Nava** | 149 | 300 | 500 | 702 | 783 | 985 | calm; = Shur intervals |
| **Rast-Panjgah** | 208 | 397 | 497 | 702 | 891 | 994 | stately; = Mahur intervals |

Notes:
- **Nava** shares Shur's intervals and **Rast-Panjgah** shares Mahur's; they are
  distinguished in practice by tonic, emphasised degrees, and *gusheh*, not by
  the bare scale — included under their own names for convenience.
- **Esfahan** is treated as a mode of **Homayoun** (Homayoun on C ≈ Esfahan on
  G, a fifth apart); use `homayoun.scl` and set the root accordingly rather than
  a separate fabricated file.
- **Shur**'s fifth degree is sometimes played natural (702) and sometimes koron;
  the default here is the natural fifth.

## Using a dastgah in code

```rust
use puget_dsp::{parse_scl, Quantizer};

let shur = parse_scl(include_str!("../scales/dastgah/shur.scl")).unwrap();
let q = Quantizer::new(shur, 220.0); // tonic (shahed) on A3
let hz = q.quantize_volts(volts);    // any V/Oct sequence -> Shur
```

## Sources

Interval data and accidental offsets drawn from published analyses of Persian
scales (Abdoli interval set; Farhat, *The Dastgah Concept in Persian Music*, on
the flexibility of the intervals):

- <https://www.microtonaltheory.com/microtonal-ethnography/persian-dastgahs>
- <https://sites.google.com/view/persianmusicscales/article>
- <https://lilypond.org/doc/v2.24/Documentation/notation/persian-classical-music>
