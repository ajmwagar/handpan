//! Render a **kamancheh** phrase in **Dastgah-e Shur**: the Persian spike
//! fiddle — a bright, nasal, vocal bowed tone over a skin-membrane body — with
//! the heavy, singing vibrato the instrument is loved for, tuned to a real
//! dastgah via the Scala quantizer + the bundled `.scl` pack.
//! `cargo run -p bowed-core --example render_kamancheh`.

use std::fs::File;
use std::io::{BufWriter, Write};

use bowed_core::{Bowed, StringKind};
use puget_dsp::{parse_scl, Quantizer};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    let shur = parse_scl(include_str!("../../../scales/dastgah/shur.scl")).expect("shur");
    let root = 293.66; // D4 tonic (shahed)
    let q = Quantizer::new(shur, root);

    let mut k = Bowed::new(FS, StringKind::Kamancheh);
    k.set_brightness(0.5);

    let mut left = Vec::new();
    let mut right = Vec::new();

    // (scale-step semitones, seconds, bow velocity, vibrato depth) — the vocal
    // kamancheh leans on deep vibrato, heaviest on the sustained notes.
    let phrase: &[(f32, f32, f32, f32)] = &[
        (0.0, 0.55, 0.7, 0.020),
        (2.0, 0.55, 0.72, 0.028),
        (3.0, 0.5, 0.72, 0.028),
        (5.0, 1.1, 0.78, 0.045), // held, deep vibrato
        (3.0, 0.5, 0.7, 0.030),
        (2.0, 0.7, 0.72, 0.035),
        (0.0, 1.2, 0.75, 0.045), // vocal quaver on the tonic
        (7.0, 0.5, 0.75, 0.025),
        (8.0, 0.55, 0.75, 0.030), // the koron 6th
        (5.0, 1.5, 0.8, 0.05),   // long, singing, wide vibrato
    ];

    let note_gap = 0.86; // bow most of the note, lift for a short tail
    for &(semi, len, bv, vib) in phrase {
        let hz = q.quantize_volts(semi / 12.0);
        k.set_vibrato(5.5, vib);
        k.note_on(hz, bv, 0.5);
        let n = (len * FS) as usize;
        let hold = (n as f32 * note_gap) as usize;
        for j in 0..n {
            if j == hold {
                k.note_off();
            }
            let s = k.process();
            left.push(s * 0.72);
            right.push(s * 0.74);
        }
    }
    for _ in 0..(2.5 * FS) as usize {
        let s = k.process();
        left.push(s * 0.72);
        right.push(s * 0.74);
    }

    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav("kamancheh_demo.wav", &left, &right, 0.89 / peak)?;
    println!("Wrote kamancheh_demo.wav: {:.1}s (Dastgah-e Shur)", left.len() as f32 / FS);
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
        for &s in &[left[i], right[i]] {
            w.write_all(&(((s * gain).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
        }
    }
    w.flush()
}
