//! Render a **barbat** (Persian *oud*) phrase in **Dastgah-e Shur**: a
//! short-neck **fretless** lute — mellow nylon/gut courses plucked with a
//! *risha* over a big, deep wooden bowl — leaning on its signature fretless
//! **glide** between notes, tuned to a real dastgah via the Scala quantizer +
//! the bundled `.scl` pack.
//! `cargo run -p plucked-core --example render_barbat`.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use plucked_core::Barbat;
use puget_dsp::{parse_scl, Quantizer};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    let scale = parse_scl(include_str!("../../../scales/dastgah/shur.scl")).expect("shur");
    let root = 220.0; // A3 tonic — a deep, warm register for the oud
    let q = Quantizer::new(scale, root);

    let mut b = Barbat::new(FS);
    b.set_brightness(0.35); // dark, round nylon voice
    b.set_sustain(0.7);
    b.set_chorus(4.0);
    b.set_width(0.35);
    b.set_glide(0.35); // a musical portamento
    b.set_humanize(0.3);

    let mut out_l = Vec::new();
    let mut out_r = Vec::new();

    // (from-step, to-step, seconds) — `from == to` is a plain pluck; otherwise
    // the note slides fretlessly from `from` to `to` (the oud's signature).
    let phrase: &[(f32, f32, f32)] = &[
        (0.0, 0.0, 0.5), (2.0, 3.0, 0.55), (3.0, 3.0, 0.45),
        (4.0, 4.0, 0.5), (5.0, 4.0, 0.6),
        (3.0, 3.0, 0.45), (2.0, 1.0, 0.6),
        (0.0, 0.0, 0.5), (3.0, 4.0, 0.5), (5.0, 5.0, 0.5),
        (7.0, 8.0, 0.9),
        (7.0, 7.0, 0.4), (5.0, 5.0, 0.45), (4.0, 3.0, 0.55),
        (1.0, 0.0, 1.4),
    ];

    for (idx, &(from, to, len)) in phrase.iter().enumerate() {
        let from_hz = q.quantize_volts(from / 12.0);
        let vel = if idx % 2 == 0 { 0.85 } else { 0.72 };
        if (from - to).abs() < 1e-6 {
            b.pluck(from_hz, vel);
        } else {
            let to_hz = q.quantize_volts(to / 12.0);
            b.pluck_glide(from_hz, to_hz, vel);
        }
        let n = (len * FS) as usize;
        for _ in 0..n {
            let (l, r) = b.process();
            out_l.push(l);
            out_r.push(r);
        }
    }
    for _ in 0..(3.0 * FS) as usize {
        let (l, r) = b.process();
        out_l.push(l);
        out_r.push(r);
    }

    let peak = out_l
        .iter()
        .chain(out_r.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav("barbat_demo.wav", &out_l, &out_r, 0.89 / peak)?;
    println!("Wrote barbat_demo.wav: {:.1}s (Dastgah-e Shur)", out_l.len() as f32 / FS);
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
