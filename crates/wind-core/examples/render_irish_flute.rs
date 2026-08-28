//! Render a flowing **Irish (wooden) flute** reel in **G major** — the warm,
//! woody, breathy folk flute: rounder and airier than the silver concert flute.
//! Equal-tempered (no microtonal scale needed).
//! `cargo run -p wind-core --example render_irish_flute`.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use wind_core::{Wind, WindKind};

const FS: f32 = 48_000.0;
const OUT: &str = "/tmp/claude-0/-home-user-handpan/6928443b-c2eb-5baf-9092-20735a10aa33/scratchpad/irish_flute_demo.wav";

/// Equal-tempered MIDI note → Hz.
fn hz(m: i32) -> f32 {
    440.0 * 2f32.powf((m as f32 - 69.0) / 12.0)
}

fn main() -> std::io::Result<()> {
    let mut v = Wind::new(FS, WindKind::IrishFlute);
    v.set_brightness(0.4); // soft, wooden embouchure
    v.set_vibrato(5.0, 0.16); // a warm, singing flute vibrato

    let mut out = Vec::new();

    // A driving 4/4 reel in G major, kept in the flute's clean D4-up register.
    // MIDI: G4=67 A4=69 B4=71 C5=72 D5=74 E5=76 F#5=78 G5=79. (note, sixteenths).
    let sixteenth = 0.125; // ~lively reel tempo
    let reel: &[(i32, f32)] = &[
        // Bar 1: G A B C  D B G B
        (67, 2.0), (69, 2.0), (71, 2.0), (72, 2.0),
        (74, 2.0), (71, 2.0), (67, 2.0), (71, 2.0),
        // Bar 2: A G F# G  A B C A
        (69, 2.0), (67, 2.0), (66, 2.0), (67, 2.0),
        (69, 2.0), (71, 2.0), (72, 2.0), (69, 2.0),
        // Bar 3: B D G(hi) D  E D B G
        (71, 2.0), (74, 2.0), (79, 2.0), (74, 2.0),
        (76, 2.0), (74, 2.0), (71, 2.0), (67, 2.0),
        // Bar 4: A B C A  B G A(long)
        (69, 2.0), (71, 2.0), (72, 2.0), (69, 2.0),
        (71, 2.0), (67, 2.0), (69, 4.0),
        // Bar 5: D E F# G  A F# D F#
        (74, 2.0), (76, 2.0), (78, 2.0), (79, 2.0),
        (81, 2.0), (78, 2.0), (74, 2.0), (78, 2.0),
        // Bar 6: E D B G  A B G(long)
        (76, 2.0), (74, 2.0), (71, 2.0), (67, 2.0),
        (69, 2.0), (71, 2.0), (67, 6.0),
    ];

    for (i, &(m, dur)) in reel.iter().enumerate() {
        let hzf = hz(m);
        v.note_on(hzf, 0.9);
        let n = (dur * sixteenth * FS) as usize;
        let hold = (n as f32 * 0.9) as usize; // legato, connected reel phrasing
        for j in 0..n {
            let t = j as f32 / n as f32;
            // A gentle breath swell — the wooden flute's rounded dynamic.
            v.set_breath(0.86 + 0.10 * (std::f32::consts::PI * t).sin());
            if j == hold && i != reel.len() - 1 {
                v.note_off();
            }
            out.push(v.process());
        }
    }
    v.note_off();
    for _ in 0..(0.8 * FS) as usize {
        out.push(v.process());
    }

    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav(OUT, &out, 0.89 / peak)?;
    println!("Wrote {OUT}: {:.1}s (G-major reel, Irish flute)", out.len() as f32 / FS);
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
