//! Render a clarinet montage — a melodic phrase across the chalumeau and
//! clarion registers, with breath swells and vibrato, plus a sustained low
//! note to show the hollow odd-harmonic body.
//! `cargo run -p wind-core --example render_winds`.

use std::fs::File;
use std::io::{BufWriter, Write};

use wind_core::{Wind, WindKind};

const FS: f32 = 48_000.0;

fn midi(n: i32) -> f32 {
    440.0 * 2f32.powf((n as f32 - 69.0) / 12.0)
}

fn main() -> std::io::Result<()> {
    let mut out = Vec::new();

    let mut v = Wind::new(FS, WindKind::Clarinet);
    v.set_vibrato(5.0, 0.18);
    v.set_brightness(0.55);

    // A little phrase, then a held low note.
    let phrase: &[(i32, f32)] = &[
        (50, 0.5),
        (53, 0.5),
        (57, 0.5),
        (58, 0.5),
        (57, 0.5),
        (53, 0.5),
        (55, 0.35),
        (57, 0.35),
        (58, 0.35),
        (62, 0.9), // up into the clarion register
        (38, 1.6), // low chalumeau — hollow and woody
    ];

    for (i, &(m, len)) in phrase.iter().enumerate() {
        let note_n = (len * FS) as usize;
        let hold = (note_n as f32 * 0.88) as usize;
        // Slightly more breath on the accented long notes.
        let breath = if len > 0.8 { 0.95 } else { 0.85 };
        v.note_on(midi(m), breath);
        for j in 0..note_n {
            let t = j as f32 / note_n as f32;
            // Gentle breath swell within each note.
            v.set_breath(breath * (0.75 + 0.25 * (std::f32::consts::PI * t).sin()));
            if j == hold && i != phrase.len() - 1 {
                v.note_off();
            }
            out.push(v.process());
        }
    }
    v.note_off();
    for _ in 0..(1.2 * FS) as usize {
        out.push(v.process());
    }

    let peak = out.iter().fold(0.0f32, |mx, &x| mx.max(x.abs())).max(1e-9);
    write_wav("winds_demo.wav", &out, 0.89 / peak)?;
    println!("Wrote winds_demo.wav: {:.1}s, peak {peak:.3}", out.len() as f32 / FS);
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
