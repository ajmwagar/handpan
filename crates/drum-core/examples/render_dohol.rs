//! Render a **dohol** groove: the Persian bass drum — deep centre booms (heavy
//! beater) answered by sharp rim cracks (the thin switch), with the head's
//! tension pitch-drop on the hard hits.
//! `cargo run -p drum-core --example render_dohol`.

use std::fs::File;
use std::io::{BufWriter, Write};

use drum_core::Dohol;

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    let mut d = Dohol::new(FS);
    d.set_tune(68.0);
    d.set_decay(0.65);
    d.set_pitch_drop(0.22);
    d.set_beater(0.55);
    d.set_drive(0.3);

    // A 6/8-ish processional pattern: (time-in-beats, velocity, position).
    // Low booms at the centre (pos ~0.1), cracks at the rim (pos ~0.9).
    let beat = 0.32;
    let pattern: &[(f32, f32, f32)] = &[
        (0.0, 1.00, 0.10), // BOOM
        (1.0, 0.55, 0.92), // ka (rim)
        (1.5, 0.50, 0.92), // ka
        (2.0, 0.85, 0.12), // Boom
        (3.0, 0.55, 0.92), // ka
        (3.5, 0.9, 0.10),  // Boom
        (4.0, 0.5, 0.92),  // ka
        (4.5, 0.5, 0.92),  // ka
        (5.0, 0.7, 0.12),  // boom
        (5.5, 0.45, 0.92), // ka
    ];
    let bars = 4;
    let bar_beats = 6.0;

    let mut out_l = Vec::new();
    let mut out_r = Vec::new();
    let total_beats = bars as f32 * bar_beats;
    let total = (total_beats * beat * FS) as usize;

    // Schedule strikes onto a sample timeline.
    let mut hits: Vec<(usize, f32, f32)> = Vec::new();
    for b in 0..bars {
        for &(t, vel, pos) in pattern {
            let when = ((b as f32 * bar_beats + t) * beat * FS) as usize;
            hits.push((when, vel, pos));
        }
    }
    hits.sort_by_key(|h| h.0);

    let mut hi = 0;
    for i in 0..total + (2.0 * FS) as usize {
        while hi < hits.len() && hits[hi].0 == i {
            d.strike(hits[hi].1, hits[hi].2);
            hi += 1;
        }
        let (l, r) = d.process();
        out_l.push(l);
        out_r.push(r);
        let _ = total;
    }

    let peak = out_l
        .iter()
        .chain(out_r.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav("dohol_groove.wav", &out_l, &out_r, 0.89 / peak)?;
    println!("Wrote dohol_groove.wav: {:.1}s (dohol bass-drum groove)", out_l.len() as f32 / FS);
    Ok(())
}

fn write_wav(path: &str, left: &[f32], right: &[f32], gain: f32) -> std::io::Result<()> {
    let n = left.len();
    let (ch, bits, sr) = (2u16, 16u16, FS as u32);
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
    for i in 0..n {
        for &sm in &[left[i], right[i]] {
            w.write_all(&(((sm * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
        }
    }
    w.flush()
}
