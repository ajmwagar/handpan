//! Render a **basitar** riff: a two-string bass/guitar hybrid hammering out
//! power-chord basslines with a bit of amp grind — root notes on the low string
//! with a fifth stacked on top, exactly one pick sweep per hit.
//! `cargo run -p plucked-core --example render_basitar`.

use std::fs::File;
use std::io::{BufWriter, Write};

use plucked_core::Basitar;

const FS: f32 = 48_000.0;

// Note names -> Hz (low register). Roots for the riff.
fn hz(semitones_from_a2: f32) -> f32 {
    110.0 * 2f32.powf(semitones_from_a2 / 12.0)
}

fn main() -> std::io::Result<()> {
    let mut b = Basitar::new(FS);
    b.set_interval(7.0); // power chord = root + fifth
    b.set_drive(0.45);
    b.set_brightness(0.6);
    b.set_sustain(0.55);

    // A driving riff: (root semitones from A2, beats). ~132 BPM eighth-ish feel.
    let beat = 0.22;
    let riff: &[(f32, f32)] = &[
        (0.0, 1.0),   // A
        (0.0, 0.5),
        (0.0, 0.5),
        (3.0, 1.0),   // C
        (5.0, 1.0),   // D
        (0.0, 1.0),   // A
        (0.0, 0.5),
        (-2.0, 0.5),  // G
        (3.0, 1.0),   // C
        (2.0, 1.0),   // B
        (0.0, 2.0),   // A (let ring)
    ];

    let mut out_l = Vec::new();
    let mut out_r = Vec::new();

    // Play the riff twice.
    for _ in 0..2 {
        for &(root, beats) in riff {
            b.pluck(hz(root), 0.95);
            let n = (beats * beat * FS) as usize;
            for _ in 0..n {
                let (l, r) = b.process();
                out_l.push(l);
                out_r.push(r);
            }
        }
    }
    // Ring out the final chord.
    for _ in 0..(2.5 * FS) as usize {
        let (l, r) = b.process();
        out_l.push(l);
        out_r.push(r);
    }

    let peak = out_l
        .iter()
        .chain(out_r.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav("basitar_riff.wav", &out_l, &out_r, 0.89 / peak)?;
    println!("Wrote basitar_riff.wav: {:.1}s (power-chord riff)", out_l.len() as f32 / FS);
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
