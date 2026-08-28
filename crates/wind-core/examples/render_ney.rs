//! Render a **ney** phrase in **Dastgah-e Shur**: the Persian end-blown reed
//! flute — a breathy, hollow, throaty jet edge tone, phrased with soft attacks
//! and an intimate air-noise onset — tuned to a real dastgah via the Scala
//! quantizer + the bundled `.scl` pack.
//! `cargo run -p wind-core --example render_ney`.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use puget_dsp::{parse_scl, Quantizer};
use wind_core::{Wind, WindKind};

const FS: f32 = 48_000.0;
const OUT: &str = "/tmp/claude-0/-home-user-handpan/6928443b-c2eb-5baf-9092-20735a10aa33/scratchpad/ney_demo.wav";

fn main() -> std::io::Result<()> {
    let scale = parse_scl(include_str!("../../../scales/dastgah/shur.scl")).expect("shur");
    // Root on C4 — the ney's stable, in-tune register sits C4 and up.
    let root = 261.63;
    let q = Quantizer::new(scale, root);

    let mut v = Wind::new(FS, WindKind::Ney);
    v.set_brightness(0.45); // soft, hollow embouchure
    v.set_vibrato(5.5, 0.18); // a gentle throat vibrato

    let mut out = Vec::new();

    // (scale-step semitones above the C4 root, seconds) — an introspective Shur
    // line that lingers on the shahed (the 4th degree) the way a ney player does.
    let phrase: &[(f32, f32)] = &[
        (0.0, 0.55), (2.0, 0.45), (3.0, 0.7), (2.0, 0.4), (0.0, 0.6),
        (3.0, 0.5), (4.0, 1.1), (3.0, 0.45), (2.0, 0.45),
        (4.0, 0.5), (5.0, 0.5), (6.0, 0.9), (5.0, 0.4), (4.0, 0.5),
        (3.0, 0.55), (2.0, 0.5), (0.0, 1.6),
    ];

    for (i, &(step, len)) in phrase.iter().enumerate() {
        let hz = q.quantize_volts(step / 12.0);
        v.note_on(hz, 0.9);
        let n = (len * FS) as usize;
        let hold = (n as f32 * 0.86) as usize;
        for j in 0..n {
            let t = j as f32 / n as f32;
            // A soft breath swell into each note (the ney's gentle dynamic).
            v.set_breath(0.88 * (0.82 + 0.18 * (std::f32::consts::PI * t).sin()));
            if j == hold && i != phrase.len() - 1 {
                v.note_off();
            }
            out.push(v.process());
        }
    }
    v.note_off();
    for _ in 0..(1.0 * FS) as usize {
        out.push(v.process());
    }

    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav(OUT, &out, 0.89 / peak)?;
    println!("Wrote {OUT}: {:.1}s (Dastgah-e Shur)", out.len() as f32 / FS);
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
