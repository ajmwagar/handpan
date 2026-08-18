//! Render a bowed-string montage (violin, viola, cello, bass) with bow
//! attacks, vibrato, and releases. `cargo run -p bowed-core --example render_strings`.

use std::fs::File;
use std::io::{BufWriter, Write};

use bowed_core::{Bowed, StringKind};

const FS: f32 = 48_000.0;

fn midi(n: i32) -> f32 {
    440.0 * 2f32.powf((n as f32 - 69.0) / 12.0)
}

fn main() -> std::io::Result<()> {
    let mut left = Vec::new();
    let mut right = Vec::new();

    // (instrument, pan, notes, note length s, bow velocity, pressure, vibrato)
    let parts: &[(StringKind, f32, &[i32], f32, f32, f32, f32)] = &[
        (StringKind::Violin, 0.4, &[76, 79, 81, 83, 81, 79, 76], 0.7, 0.75, 0.5, 0.02),
        (StringKind::Viola, 0.1, &[64, 67, 69, 71, 69, 67], 0.8, 0.7, 0.5, 0.015),
        (StringKind::Cello, -0.2, &[48, 52, 55, 57, 55, 52], 1.0, 0.65, 0.55, 0.012),
        (StringKind::Bass, -0.4, &[31, 36, 38, 36], 1.2, 0.6, 0.6, 0.008),
    ];

    for (kind, pan, notes, len, bv, bp, vib) in parts {
        let mut v = Bowed::new(FS, *kind);
        v.set_vibrato(5.5, *vib);
        let total_s = notes.len() as f32 * *len + 1.5;
        let n = (total_s * FS) as usize;
        let base = left.len();
        left.resize(base + n, 0.0);
        right.resize(base + n, 0.0);
        let theta = (*pan + 1.0) * 0.25 * std::f32::consts::PI;
        let (pl, pr) = (theta.cos(), theta.sin());

        let note_n = (*len * FS) as usize;
        let hold = (note_n as f32 * 0.82) as usize; // bow, then lift for the tail
        for (k, &m) in notes.iter().enumerate() {
            v.note_on(midi(m), *bv, *bp);
            for j in 0..note_n {
                if j == hold {
                    v.note_off();
                }
                let s = v.process();
                let idx = base + k * note_n + j;
                if idx < left.len() {
                    left[idx] += s * pl;
                    right[idx] += s * pr;
                }
            }
        }
        // ring out the last note
        for j in 0..(1.5 * FS) as usize {
            let s = v.process();
            let idx = base + notes.len() * note_n + j;
            if idx < left.len() {
                left[idx] += s * pl;
                right[idx] += s * pr;
            }
        }
    }

    // --- Pizzicato coda: a plucked cello + bass walking line ---------------
    {
        let base = left.len();
        let voices: &[(StringKind, f32, &[i32], f32)] = &[
            (StringKind::Cello, -0.2, &[48, 55, 52, 57, 55, 60, 55, 52], 0.42),
            (StringKind::Bass, -0.4, &[36, 36, 43, 43, 41, 41, 36, 36], 0.42),
        ];
        let total = voices.iter().map(|(_, _, n, l)| n.len() as f32 * l).fold(0.0, f32::max) + 2.0;
        let nn = (total * FS) as usize;
        left.resize(base + nn, 0.0);
        right.resize(base + nn, 0.0);
        for (kind, pan, notes, len) in voices {
            let mut v = Bowed::new(FS, *kind);
            let theta = (*pan + 1.0) * 0.25 * std::f32::consts::PI;
            let (pl, pr) = (theta.cos(), theta.sin());
            let note_n = (*len * FS) as usize;
            for (k, &m) in notes.iter().enumerate() {
                v.pluck(midi(m), 0.85);
                for j in 0..note_n {
                    let s = v.process();
                    let idx = base + k * note_n + j;
                    if idx < left.len() {
                        left[idx] += s * pl;
                        right[idx] += s * pr;
                    }
                }
            }
            // ring out
            for j in 0..(2.0 * FS) as usize {
                let s = v.process();
                let idx = base + notes.len() * note_n + j;
                if idx < left.len() {
                    left[idx] += s * pl;
                    right[idx] += s * pr;
                }
            }
        }
    }

    // --- Sul tasto → sul ponticello sweep on a sustained violin note ------
    {
        let base = left.len();
        let nn = (4.0 * FS) as usize;
        left.resize(base + nn, 0.0);
        right.resize(base + nn, 0.0);
        let mut v = Bowed::new(FS, StringKind::Violin);
        v.set_vibrato(5.5, 0.015);
        v.note_on(midi(69), 0.7, 0.5); // A4
        for j in 0..nn {
            let t = j as f32 / nn as f32;
            // Sweep the bow from over the fingerboard toward the bridge.
            v.set_bow_position(t);
            v.set_brightness(0.2 + 0.7 * t);
            if j == (nn as f32 * 0.8) as usize {
                v.note_off();
            }
            let s = v.process();
            left[base + j] += s * 0.7;
            right[base + j] += s * 0.72;
        }
    }

    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |mx, &x| mx.max(x.abs()))
        .max(1e-9);
    write_wav("strings_demo.wav", &left, &right, 0.89 / peak)?;
    println!("Wrote strings_demo.wav: {:.1}s, peak {peak:.3}", left.len() as f32 / FS);
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
