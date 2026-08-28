//! Render the mallet "chairs" section: a marimba pattern played solo, then by a
//! tight-tuned section, then by a wide-spread section — so you can hear a single
//! player grow into a whole row of mallets, and hear the spread knob open up.
//! `cargo run -p mallet-core --example render_section`.

use std::fs::File;
use std::io::{BufWriter, Write};

use mallet_core::{Instrument, MalletEnsemble};

const FS: f32 = 48_000.0;

fn play(l: &mut Vec<f32>, r: &mut Vec<f32>, chairs: usize, spread: f32) {
    let mut sec = MalletEnsemble::new(FS, Instrument::Marimba, 10, spread, 6);
    sec.set_chairs(chairs);
    // A rolling pattern.
    let pattern = [60, 64, 67, 72, 67, 64, 60, 55, 60, 64, 67, 72];
    let step = (0.16 * FS) as usize;
    let mut t = 0usize;
    let total = pattern.len() * step + (2.5 * FS) as usize;
    let base = l.len();
    l.resize(base + total, 0.0);
    r.resize(base + total, 0.0);
    for (k, &note) in pattern.iter().enumerate() {
        sec.strike_midi(note as f32, 0.9);
        let start = k * step;
        // render this step's worth into the buffer (overlapping tails handled by
        // rendering the whole remaining span each strike would double-count, so
        // we render forward only `step` here and a long tail after the last).
        for j in 0..step {
            let (sl, sr) = sec.process();
            l[base + start + j] += sl;
            r[base + start + j] += sr;
        }
        t = start + step;
    }
    for j in 0..(2.5 * FS) as usize {
        let (sl, sr) = sec.process();
        if base + t + j < l.len() {
            l[base + t + j] += sl;
            r[base + t + j] += sr;
        }
    }
}

fn main() -> std::io::Result<()> {
    let mut l = Vec::new();
    let mut r = Vec::new();

    play(&mut l, &mut r, 1, 5.0); // solo
    play(&mut l, &mut r, 8, 4.0); // tight section
    play(&mut l, &mut r, 8, 12.0); // wide-spread section

    let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav("mallet_section_demo.wav", &l, &r, 0.89 / peak)?;
    println!(
        "Wrote mallet_section_demo.wav: {:.1}s (solo, 8 tight, 8 wide-spread)",
        l.len() as f32 / FS
    );
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
