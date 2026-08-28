//! Render a lively **tin whistle** jig in **D major** — the bright, pure,
//! slightly shrill penny whistle, with its clean two-register voice and the
//! chiff on every attack. Equal-tempered (no microtonal scale needed).
//! `cargo run -p wind-core --example render_tin_whistle`.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use wind_core::{Wind, WindKind};

const FS: f32 = 48_000.0;
const OUT: &str = "/tmp/claude-0/-home-user-handpan/6928443b-c2eb-5baf-9092-20735a10aa33/scratchpad/tin_whistle_demo.wav";

/// Equal-tempered MIDI note → Hz.
fn hz(m: i32) -> f32 {
    440.0 * 2f32.powf((m as f32 - 69.0) / 12.0)
}

fn main() -> std::io::Result<()> {
    let mut v = Wind::new(FS, WindKind::TinWhistle);
    v.set_brightness(0.7); // bright, shrill penny-whistle edge
    v.set_vibrato(5.5, 0.12); // a light finger/breath vibrato

    let mut out = Vec::new();

    // A bouncing 6/8 jig in D major, sitting in the whistle's clean upper octave
    // (D4-up register). MIDI numbers: D4=62 … D5=74 … E5=76. (note, eighths).
    // Two-note pickup, then rolling jig phrases.
    let eighth = 0.19; // seconds per eighth note (~lively jig tempo)
    let jig: &[(i32, f32)] = &[
        // Bar 1: D E F# | G F# E
        (74, 1.0), (73, 1.0), (71, 1.0), (69, 1.0), (71, 1.0), (73, 1.0),
        // Bar 2: F# D D | A D D  (a classic jig lilt)
        (74, 1.0), (69, 1.0), (69, 1.0), (76, 1.0), (74, 1.0), (74, 1.0),
        // Bar 3: B A G | F# A F#
        (83, 1.0), (81, 1.0), (79, 1.0), (78, 1.0), (81, 1.0), (78, 1.0),
        // Bar 4: E F# G | F# E D
        (76, 1.0), (78, 1.0), (79, 1.0), (78, 1.0), (76, 1.0), (74, 1.5),
        // Bar 5: A A A | B A G
        (81, 1.0), (81, 1.0), (81, 1.0), (83, 1.0), (81, 1.0), (79, 1.0),
        // Bar 6: F# D E | F# G A
        (78, 1.0), (74, 1.0), (76, 1.0), (78, 1.0), (79, 1.0), (81, 1.0),
        // Bar 7: B A F# | E F# E
        (83, 1.0), (81, 1.0), (78, 1.0), (76, 1.0), (78, 1.0), (76, 1.0),
        // Bar 8: D E F# | D (long)
        (74, 1.0), (76, 1.0), (78, 1.0), (74, 3.0),
    ];

    for (i, &(m, dur)) in jig.iter().enumerate() {
        let hzf = hz(m);
        v.note_on(hzf, 0.92);
        let n = (dur * eighth * FS) as usize;
        let hold = (n as f32 * 0.82) as usize; // crisp, detached jig articulation
        for j in 0..n {
            let t = j as f32 / n as f32;
            v.set_breath(0.90 + 0.06 * (std::f32::consts::PI * t).sin());
            if j == hold && i != jig.len() - 1 {
                v.note_off();
            }
            out.push(v.process());
        }
    }
    v.note_off();
    for _ in 0..(0.6 * FS) as usize {
        out.push(v.process());
    }

    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav(OUT, &out, 0.89 / peak)?;
    println!("Wrote {OUT}: {:.1}s (D-major jig, tin whistle)", out.len() as f32 / FS);
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
