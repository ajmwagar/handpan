//! Render a **daf** phrase: the large Persian frame drum — deep open *dum* tones
//! at the centre and crisp *tak* strokes at the rim, each setting the internal
//! metal rings shimmering, closing on a held *shake* flourish that rattles the
//! jingles on their own.
//! `cargo run -p drum-core --example render_daf`.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use drum_core::Daf;

const FS: f32 = 48_000.0;

// A stroke on the timeline: (beat, kind, velocity). kind: D=dum, T=tak, S=shake.
#[derive(Clone, Copy)]
enum K {
    D,
    T,
    S,
}

fn main() -> std::io::Result<()> {
    let mut d = Daf::new(FS);
    d.set_tune(84.0);
    d.set_decay(0.55);
    d.set_jingle(0.8);

    let beat = 0.27;
    // A daf frame-drum groove: dum on the downbeats, tak filling the rim.
    let pattern: &[(f32, K, f32)] = &[
        (0.0, K::D, 1.00),
        (1.0, K::T, 0.70),
        (1.5, K::T, 0.55),
        (2.0, K::D, 0.85),
        (2.5, K::T, 0.65),
        (3.0, K::D, 0.90),
        (3.5, K::T, 0.60),
        (4.0, K::T, 0.75),
        (4.5, K::T, 0.55),
        (5.0, K::D, 0.80),
        (5.5, K::T, 0.70),
    ];
    let bars = 3;
    let bar_beats = 6.0;

    let mut hits: Vec<(usize, K, f32)> = Vec::new();
    for b in 0..bars {
        for &(t, k, v) in pattern {
            hits.push((((b as f32 * bar_beats + t) * beat * FS) as usize, k, v));
        }
    }
    // A closing shake flourish: two swelling shakes over the last bar's tail.
    let shake_start = (bars as f32 * bar_beats * beat * FS) as usize;
    hits.push((shake_start, K::S, 0.7));
    hits.push((shake_start + (0.9 * FS) as usize, K::S, 1.0));
    // A final accented dum to land the phrase.
    let last = shake_start + (2.0 * FS) as usize;
    hits.push((last, K::D, 1.0));
    hits.sort_by_key(|h| h.0);

    let total = hits.last().map(|h| h.0).unwrap_or(0) + (2.5 * FS) as usize;
    let mut out = Vec::with_capacity(total);
    let mut hi = 0;
    for i in 0..total {
        while hi < hits.len() && hits[hi].0 == i {
            match hits[hi].1 {
                K::D => d.dum(hits[hi].2),
                K::T => d.tak(hits[hi].2),
                K::S => d.shake(hits[hi].2),
            }
            hi += 1;
        }
        out.push(d.process().0);
    }

    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav("daf_groove.wav", &out, 0.89 / peak)?;
    println!("Wrote daf_groove.wav: {:.1}s (daf phrase + shake flourish)", out.len() as f32 / FS);
    Ok(())
}

fn write_wav(path: &str, mono: &[f32], gain: f32) -> std::io::Result<()> {
    let n = mono.len();
    let (ch, bits, sr) = (1u16, 16u16, FS as u32);
    let ba = ch * bits / 8;
    let dl = (n as u32) * ba as u32;
    let mut w = BufWriter::new(File::create(path)?);
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + dl).to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&ch.to_le_bytes())?;
    w.write_all(&sr.to_le_bytes())?;
    w.write_all(&(sr * ba as u32).to_le_bytes())?;
    w.write_all(&ba.to_le_bytes())?;
    w.write_all(&bits.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&dl.to_le_bytes())?;
    for &s in mono {
        w.write_all(&(((s * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    w.flush()
}
