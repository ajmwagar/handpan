//! Render the handpan "choir": a gentle phrase played by a growing section —
//! solo, then 6 chairs, then 14 — so you can hear one handpan bloom into a
//! whole shimmering wash. `cargo run -p handpan-core --example render_choir`.

use std::fs::File;
use std::io::{BufWriter, Write};

use handpan_core::{Build, HandpanEnsemble, Scale, Size};

const FS: f32 = 48_000.0;

fn play(l: &mut Vec<f32>, r: &mut Vec<f32>, chairs: usize, spread: f32) {
    let mut choir = HandpanEnsemble::new(FS, &Scale::DKurd9, Build::Handpan, Size::Standard, 14, spread);
    choir.set_chairs(chairs);
    choir.set_air(0.25);
    choir.set_coupling(0.08);
    let n = choir.note_count();
    // A slow, meditative pattern across the fields.
    let pattern = [0, 2, 4, 3, 5, 2, 1, 4, 6, 3];
    let step = (0.55 * FS) as usize;
    let base = l.len();
    let total = pattern.len() * step + (4.0 * FS) as usize;
    l.resize(base + total, 0.0);
    r.resize(base + total, 0.0);
    for (k, &field) in pattern.iter().enumerate() {
        choir.strike(field % n, 0.85);
        let start = k * step;
        for j in 0..step {
            let (sl, sr) = choir.process();
            l[base + start + j] += sl;
            r[base + start + j] += sr;
        }
    }
    let t = pattern.len() * step;
    for j in 0..(4.0 * FS) as usize {
        let (sl, sr) = choir.process();
        if base + t + j < l.len() {
            l[base + t + j] += sl;
            r[base + t + j] += sr;
        }
    }
}

fn main() -> std::io::Result<()> {
    let mut l = Vec::new();
    let mut r = Vec::new();

    play(&mut l, &mut r, 1, 6.0); // solo
    play(&mut l, &mut r, 6, 6.0); // section
    play(&mut l, &mut r, 14, 9.0); // full choir, a touch wider

    let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav("handpan_choir_demo.wav", &l, &r, 0.89 / peak)?;
    println!("Wrote handpan_choir_demo.wav: {:.1}s (solo, 6 chairs, 14 chairs)", l.len() as f32 / FS);
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
