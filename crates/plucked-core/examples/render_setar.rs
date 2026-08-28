//! Render a **setar** phrase in **Dastgah-e Homayoun**: the small, delicate
//! Persian long-neck lute — thin fingernail-plucked strings over a warm
//! mulberry-wood box, with a sympathetic bass string humming beneath — tuned to
//! a real dastgah via the Scala quantizer + the bundled `.scl` pack.
//! `cargo run -p plucked-core --example render_setar`.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use plucked_core::Setar;
use puget_dsp::{parse_scl, Quantizer};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    let scale = parse_scl(include_str!("../../../scales/dastgah/homayoun.scl")).expect("homayoun");
    let root = 261.63; // C4 tonic
    let q = Quantizer::new(scale, root);

    let mut s = Setar::new(FS);
    s.set_brightness(0.4); // soft, dark fingernail voice
    s.set_sustain(0.75);
    s.set_width(0.3);
    // Tune the sympathetic bass an octave below the tonic — a warm drone halo.
    s.set_sympathetic_hz(root * 0.5);
    s.set_humanize(0.3);

    let mut out_l = Vec::new();
    let mut out_r = Vec::new();

    // Open the phrase by softly sounding the sympathetic bass, then let the
    // melody bloom over its ringing halo.
    s.pluck_sympathetic(0.55);

    // (scale-step semitones, seconds) — an intimate, unhurried melodic line.
    let phrase: &[(f32, f32)] = &[
        (0.0, 0.45), (1.0, 0.45), (3.0, 0.45), (4.0, 0.6),
        (3.0, 0.4), (1.0, 0.7),
        (4.0, 0.45), (5.0, 0.45), (7.0, 0.6),
        (8.0, 0.9),
        (7.0, 0.4), (5.0, 0.4), (4.0, 0.45), (3.0, 0.45),
        (1.0, 0.5), (0.0, 1.4),
    ];

    for (idx, &(semi, len)) in phrase.iter().enumerate() {
        let hz = q.quantize_volts(semi / 12.0);
        s.pluck(hz, if idx % 2 == 0 { 0.8 } else { 0.68 });
        // Re-sound the bass halo occasionally under the melody.
        if idx == 5 || idx == 11 {
            s.pluck_sympathetic(0.5);
        }
        let n = (len * FS) as usize;
        for _ in 0..n {
            let (l, r) = s.process();
            out_l.push(l);
            out_r.push(r);
        }
    }
    for _ in 0..(3.0 * FS) as usize {
        let (l, r) = s.process();
        out_l.push(l);
        out_r.push(r);
    }

    let peak = out_l
        .iter()
        .chain(out_r.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav("setar_demo.wav", &out_l, &out_r, 0.89 / peak)?;
    println!("Wrote setar_demo.wav: {:.1}s (Dastgah-e Homayoun)", out_l.len() as f32 / FS);
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
