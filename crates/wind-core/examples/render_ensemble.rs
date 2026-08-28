//! Render the "chairs" ensemble: the same phrase played by a growing section —
//! 1 player, then 4, then 12 — so you can hear the solo bloom into a whole
//! massive humanized wind section. `cargo run -p wind-core --example render_ensemble`.

use std::fs::File;
use std::io::{BufWriter, Write};

use wind_core::{WindEnsemble, WindKind};

const FS: f32 = 48_000.0;

fn midi(n: i32) -> f32 {
    440.0 * 2f32.powf((n as f32 - 69.0) / 12.0)
}

fn play(l: &mut Vec<f32>, r: &mut Vec<f32>, kind: WindKind, chairs: usize, notes: &[(i32, f32)], breath: f32) {
    let mut sec = WindEnsemble::new(FS, kind, 12, 8.0);
    sec.set_chairs(chairs);
    sec.set_vibrato(5.0, 0.14);
    sec.set_brightness(0.55);
    for (i, &(m, len)) in notes.iter().enumerate() {
        let n = (len * FS) as usize;
        let hold = (n as f32 * 0.88) as usize;
        sec.note_on(midi(m), breath);
        for j in 0..n {
            if j == hold && i != notes.len() - 1 {
                sec.note_off();
            }
            let (sl, sr) = sec.process();
            l.push(sl);
            r.push(sr);
        }
    }
    sec.note_off();
    for _ in 0..(1.2 * FS) as usize {
        let (sl, sr) = sec.process();
        l.push(sl);
        r.push(sr);
    }
}

fn main() -> std::io::Result<()> {
    let mut l = Vec::new();
    let mut r = Vec::new();

    // A stately clarinet line, played by 1 → 4 → 12 chairs.
    let phrase = [(62, 0.7), (65, 0.7), (69, 0.9), (67, 0.7), (65, 0.7), (62, 1.4)];
    for &chairs in &[1usize, 4, 12] {
        play(&mut l, &mut r, WindKind::Clarinet, chairs, &phrase, 0.9);
    }

    let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav("ensemble_demo.wav", &l, &r, 0.89 / peak)?;
    println!("Wrote ensemble_demo.wav: {:.1}s (solo, 4 chairs, 12 chairs)", l.len() as f32 / FS);
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
