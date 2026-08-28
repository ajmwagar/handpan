//! Render a didgeridoo drone: a low D drone with the vocal-formant "wah" wobble,
//! a rhythmic pulse, and a mouth-shape (Timbre) sweep — then a faster wobble.
//! `cargo run -p wind-core --example render_didge`.

use std::fs::File;
use std::io::{BufWriter, Write};

use wind_core::{Wind, WindKind};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    let mut out = Vec::new();
    let f0 = 440.0 * 2f32.powf((38.0 - 69.0) / 12.0); // D2 ≈ 73 Hz

    let mut v = Wind::new(FS, WindKind::Didgeridoo);
    v.set_vibrato(4.5, 0.5); // "vibrato" on the didge = the wobble rate/depth
    v.note_on(f0, 0.9);

    let n = (10.0 * FS) as usize;
    for j in 0..n {
        let t = j as f32 / FS;
        // Mouth-shape (Timbre) slowly sweeps the vocal-formant centre.
        v.set_brightness(0.5 + 0.45 * (0.5 * (t * 0.5).sin() + 0.5));
        // A rhythmic breath pulse (~2 Hz) — the tongue/diaphragm accent.
        let pulse = 0.75 + 0.25 * (1.0 + (std::f32::consts::TAU * 2.0 * t).sin()).max(0.0) * 0.5;
        v.set_breath(0.9 * pulse);
        // Halfway through, speed up the wobble (a faster rhythm).
        if j == n / 2 {
            v.set_vibrato(7.5, 0.7);
        }
        out.push(v.process());
    }
    v.note_off();
    for _ in 0..(0.8 * FS) as usize {
        out.push(v.process());
    }

    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav("didge_demo.wav", &out, 0.89 / peak)?;
    println!("Wrote didge_demo.wav: {:.1}s (drone; wobble speeds up halfway)", out.len() as f32 / FS);
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
