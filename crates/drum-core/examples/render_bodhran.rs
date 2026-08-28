//! Render a **bodhrán** groove: the Irish frame drum, played with the
//! double-ended wooden *tipper* in a driving reel-into-jig feel — low centre
//! strokes and high rim strokes trading in the up-down tipper motion — while
//! the free hand works the back of the skin for the expressive pitch-bend
//! "wow".
//! `cargo run -p drum-core --example render_bodhran`.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use drum_core::Bodhran;

const FS: f32 = 48_000.0;

// A stroke: (beat, end, velocity). end: L = low/centre tipper, H = high/rim.
#[derive(Clone, Copy)]
enum E {
    L,
    H,
}

// A pressure move: (beat, target 0..1). The hand eases toward it smoothly.
type Press = (f32, f32);

fn main() -> std::io::Result<()> {
    let mut b = Bodhran::new(FS);
    b.set_tune(88.0);
    b.set_decay(0.42);

    let beat = 0.30; // seconds per 1/8 (a lively reel pulse)

    // One bar of a reel: down-up tipper triplets/eighths, accent on the pulse.
    // Beats are in eighth-note units; a bar is 8 eighths.
    let reel: &[(f32, E, f32)] = &[
        (0.0, E::L, 1.00),
        (0.5, E::H, 0.55),
        (1.0, E::L, 0.75),
        (1.5, E::H, 0.55),
        (2.0, E::L, 0.90),
        (2.5, E::H, 0.55),
        (3.0, E::L, 0.72),
        (3.5, E::H, 0.60),
        (4.0, E::L, 0.95),
        (4.5, E::H, 0.55),
        (5.0, E::L, 0.75),
        (5.5, E::H, 0.55),
        (6.0, E::L, 0.88),
        (6.5, E::H, 0.58),
        (7.0, E::L, 0.70),
        (7.5, E::H, 0.62),
    ];
    // A jig-feel bar: swung 6/8 triplets, low-low-high grouping.
    let jig: &[(f32, E, f32)] = &[
        (0.0, E::L, 1.00),
        (0.67, E::L, 0.55),
        (1.33, E::H, 0.62),
        (2.0, E::L, 0.90),
        (2.67, E::L, 0.55),
        (3.33, E::H, 0.62),
        (4.0, E::L, 0.95),
        (4.67, E::L, 0.55),
        (5.33, E::H, 0.62),
        (6.0, E::L, 0.88),
        (6.67, E::L, 0.55),
        (7.33, E::H, 0.60),
    ];

    let bar_eighths = 8.0;

    // Song form: 2 reel bars, 2 jig bars, 1 reel bar to land.
    let form: &[&[(f32, E, f32)]] = &[reel, reel, jig, jig, reel];

    // Hand-pressure automation over the whole phrase (in absolute eighth units).
    // Relaxed to open the groove, then a rising bend across bar 2 (the "wow"),
    // released for the jig, a wide bend up through bar 4, and a final press.
    let total_eighths = form.len() as f32 * bar_eighths;
    let presses: &[Press] = &[
        (0.0, 0.0),
        (bar_eighths + 4.0, 0.0),   // start of bar 2, still low
        (bar_eighths + 7.5, 0.85),  // bend up into the turnaround — the wow
        (2.0 * bar_eighths, 0.15),  // release for the jig
        (3.0 * bar_eighths + 4.0, 0.15),
        (3.0 * bar_eighths + 7.5, 0.95), // wide bend up at the jig turnaround
        (4.0 * bar_eighths, 0.35),       // settle for the final reel bar
        (4.0 * bar_eighths + 6.0, 1.0),  // press hard on the last strokes
    ];

    // Flatten strokes to sample positions.
    let mut hits: Vec<(usize, E, f32)> = Vec::new();
    for (bar, pat) in form.iter().enumerate() {
        let bar0 = bar as f32 * bar_eighths;
        for &(t, e, v) in *pat {
            let s = ((bar0 + t) * beat * FS) as usize;
            hits.push((s, e, v));
        }
    }
    hits.sort_by_key(|h| h.0);

    // Flatten pressure moves to sample positions.
    let mut pmoves: Vec<(usize, f32)> =
        presses.iter().map(|&(t, a)| ((t * beat * FS) as usize, a)).collect();
    pmoves.sort_by_key(|p| p.0);

    let end_eighths = total_eighths + 3.0; // let the tail ring and the last bend sing
    let total = (end_eighths * beat * FS) as usize;
    let mut out = Vec::with_capacity(total);
    let mut hi = 0;
    let mut pi = 0;
    for i in 0..total {
        while hi < hits.len() && hits[hi].0 == i {
            match hits[hi].1 {
                E::L => b.low(hits[hi].2),
                E::H => b.high(hits[hi].2),
            }
            hi += 1;
        }
        while pi < pmoves.len() && pmoves[pi].0 == i {
            b.set_pressure(pmoves[pi].1);
            pi += 1;
        }
        out.push(b.process().0);
    }

    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav("bodhran_groove.wav", &out, 0.89 / peak)?;
    println!(
        "Wrote bodhran_groove.wav: {:.1}s (reel/jig tipper groove with hand-pressure bends)",
        out.len() as f32 / FS
    );
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
