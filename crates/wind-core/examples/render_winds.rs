//! Render a wind montage: clarinet (woody, odd-harmonic), alto sax (reedy,
//! all-harmonic), and trumpet (brassy lip reed). Breath swells + vibrato.
//! `cargo run -p wind-core --example render_winds`.

use std::fs::File;
use std::io::{BufWriter, Write};

use wind_core::{Wind, WindKind};

const FS: f32 = 48_000.0;

fn midi(n: i32) -> f32 {
    440.0 * 2f32.powf((n as f32 - 69.0) / 12.0)
}

fn phrase(out: &mut Vec<f32>, kind: WindKind, bright: f32, vib: f32, notes: &[(i32, f32)], breath: f32) {
    let mut v = Wind::new(FS, kind);
    v.set_vibrato(5.0, vib);
    v.set_brightness(bright);
    for (i, &(m, len)) in notes.iter().enumerate() {
        let note_n = (len * FS) as usize;
        let hold = (note_n as f32 * 0.86) as usize;
        v.note_on(midi(m), breath);
        for j in 0..note_n {
            let t = j as f32 / note_n as f32;
            v.set_breath(breath * (0.78 + 0.22 * (std::f32::consts::PI * t).sin()));
            if j == hold && i != notes.len() - 1 {
                v.note_off();
            }
            out.push(v.process());
        }
    }
    v.note_off();
    for _ in 0..(0.9 * FS) as usize {
        out.push(v.process());
    }
}

fn main() -> std::io::Result<()> {
    let mut out = Vec::new();

    // Clarinet — a lyrical line into the low chalumeau.
    phrase(&mut out, WindKind::Clarinet, 0.55, 0.16,
        &[(62, 0.5), (65, 0.5), (69, 0.5), (70, 0.6), (69, 0.5), (65, 0.5), (62, 0.9), (50, 1.4)], 0.9);

    // Alto sax — a bluesy, reedy phrase in the mid register.
    phrase(&mut out, WindKind::Saxophone, 0.6, 0.25,
        &[(63, 0.45), (65, 0.3), (66, 0.3), (68, 0.5), (66, 0.4), (63, 0.4), (61, 0.7), (58, 1.2)], 0.85);

    // Trumpet — a bright fanfare.
    phrase(&mut out, WindKind::Trumpet, 0.6, 0.12,
        &[(60, 0.35), (64, 0.35), (67, 0.35), (72, 0.7), (67, 0.3), (72, 0.3), (74, 1.1), (72, 1.4)], 0.9);

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
