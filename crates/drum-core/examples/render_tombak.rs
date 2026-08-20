//! Render a **tombak** (*zarb*) phrase: the Persian goblet hand drum — deep
//! pitched *tom* tones at the centre, dry bright *bak* slaps at the rim, light
//! *finger* taps, and a closing *riz* (rapid finger roll).
//! `cargo run -p drum-core --example render_tombak`.

use std::fs::File;
use std::io::{BufWriter, Write};

use drum_core::Tombak;

const FS: f32 = 48_000.0;

// A stroke on the timeline: (beat, kind, velocity). kind: T=tom, B=bak, F=finger.
#[derive(Clone, Copy)]
enum K {
    T,
    B,
    F,
}

fn main() -> std::io::Result<()> {
    let mut d = Tombak::new(FS);
    d.set_tune(104.0);
    d.set_decay(0.42);

    let beat = 0.26;
    // A classic tombak-style pattern (two bars), then a riz roll.
    let pattern: &[(f32, K, f32)] = &[
        (0.0, K::T, 1.00),
        (1.0, K::B, 0.80),
        (1.5, K::F, 0.55),
        (2.0, K::T, 0.85),
        (2.5, K::B, 0.75),
        (3.0, K::B, 0.80),
        (3.5, K::F, 0.50),
        (4.0, K::T, 0.95),
        (4.5, K::F, 0.55),
        (5.0, K::B, 0.80),
        (5.5, K::T, 0.70),
    ];
    let bars = 3;
    let bar_beats = 6.0;

    // Schedule onto a sample timeline.
    let mut hits: Vec<(usize, K, f32)> = Vec::new();
    for b in 0..bars {
        for &(t, k, v) in pattern {
            hits.push((((b as f32 * bar_beats + t) * beat * FS) as usize, k, v));
        }
    }
    // A riz: a fast finger roll (~18 Hz), swelling, over the last bar's tail.
    let riz_start = (bars as f32 * bar_beats * beat * FS) as usize;
    let riz_len = (1.6 * FS) as usize;
    let riz_step = (FS / 18.0) as usize;
    let mut r = riz_start;
    let mut k = 0;
    while r < riz_start + riz_len {
        let swell = 0.35 + 0.5 * (k as f32 / (riz_len / riz_step) as f32);
        hits.push((r, K::F, swell));
        r += riz_step;
        k += 1;
    }
    // A final accented tom.
    hits.push((riz_start + riz_len, K::T, 1.0));
    hits.sort_by_key(|h| h.0);

    let total = hits.last().map(|h| h.0).unwrap_or(0) + (2.5 * FS) as usize;
    let mut out = Vec::with_capacity(total);
    let mut hi = 0;
    for i in 0..total {
        while hi < hits.len() && hits[hi].0 == i {
            match hits[hi].1 {
                K::T => d.tom(hits[hi].2),
                K::B => d.bak(hits[hi].2),
                K::F => d.finger(hits[hi].2),
            }
            hi += 1;
        }
        out.push(d.process().0);
    }

    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav("tombak_groove.wav", &out, 0.89 / peak)?;
    println!("Wrote tombak_groove.wav: {:.1}s (tombak phrase + riz)", out.len() as f32 / FS);
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
