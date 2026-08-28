//! Render a **sorna** phrase in **Dastgah-e Chahargah**: the Iranian double-reed
//! shawm — loud, bright, buzzy and nasal, the outdoor partner to the dohol —
//! over a simple sustained root drone, tuned to a real dastgah via the Scala
//! quantizer + the bundled `.scl` pack.
//! `cargo run -p wind-core --example render_sorna`.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use puget_dsp::{parse_scl, Quantizer};
use wind_core::{Wind, WindKind};

const FS: f32 = 48_000.0;
const OUT: &str = "/tmp/claude-0/-home-user-handpan/6928443b-c2eb-5baf-9092-20735a10aa33/scratchpad/sorna_demo.wav";

fn main() -> std::io::Result<()> {
    let scale = parse_scl(include_str!("../../../scales/dastgah/chahargah.scl")).expect("chahargah");
    let root = 196.0; // G3 tonic — a bright, projecting shawm register.
    let q = Quantizer::new(scale, root);

    // Lead shawm.
    let mut lead = Wind::new(FS, WindKind::Sorna);
    lead.set_brightness(0.7);
    lead.set_vibrato(6.0, 0.2);

    // A steady root drone (a second, quieter, darker sorna held on the tonic).
    let mut drone = Wind::new(FS, WindKind::Sorna);
    drone.set_brightness(0.3);
    drone.note_on(root, 0.7);

    let mut out = Vec::new();

    // A driving Chahargah line — the festive, martial character of the mode.
    let phrase: &[(f32, f32)] = &[
        (0.0, 0.28), (1.0, 0.28), (2.0, 0.28), (1.0, 0.28),
        (3.0, 0.5), (2.0, 0.26), (1.0, 0.26), (0.0, 0.55),
        (3.0, 0.3), (4.0, 0.3), (5.0, 0.6), (4.0, 0.28), (3.0, 0.28),
        (2.0, 0.3), (3.0, 0.3), (1.0, 0.5),
        (0.0, 0.3), (2.0, 0.3), (4.0, 0.3), (5.0, 0.3), (7.0, 0.9),
        (5.0, 0.3), (3.0, 0.3), (0.0, 1.4),
    ];

    for (i, &(step, len)) in phrase.iter().enumerate() {
        let hz = q.quantize_volts(step / 12.0);
        lead.note_on(hz, 0.9);
        let n = (len * FS) as usize;
        let hold = (n as f32 * 0.9) as usize;
        for j in 0..n {
            // Keep the drone breathing steadily underneath.
            drone.set_breath(0.7);
            if j == hold && i != phrase.len() - 1 {
                lead.note_off();
            }
            let s = 0.8 * lead.process() + 0.35 * drone.process();
            out.push(s);
        }
    }
    lead.note_off();
    drone.note_off();
    for _ in 0..(1.0 * FS) as usize {
        out.push(0.8 * lead.process() + 0.35 * drone.process());
    }

    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav(OUT, &out, 0.89 / peak)?;
    println!("Wrote {OUT}: {:.1}s (Dastgah-e Chahargah, shawm + drone)", out.len() as f32 / FS);
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
