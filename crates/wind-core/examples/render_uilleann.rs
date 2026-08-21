//! Render an **uilleann pipes** jig in **D major** — the bright, sweet, nasal
//! reed **chanter** singing over the pipes' constant **drones** (the tonic D,
//! sounded in octaves underneath). The chanter and the drone bank are the one
//! `Wind` voice: `set_drone` sounds the tonic bed, and it keeps droning between
//! chanter notes, exactly like the real instrument. The chordal regulators are
//! out of scope. Equal-tempered (no microtonal scale needed).
//! `cargo run -p wind-core --example render_uilleann`.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use wind_core::{Wind, WindKind};

const FS: f32 = 48_000.0;
const OUT: &str = "/tmp/claude-0/-home-user-handpan/6928443b-c2eb-5baf-9092-20735a10aa33/scratchpad/uilleann_demo.wav";

/// Equal-tempered MIDI note → Hz.
fn hz(m: i32) -> f32 {
    440.0 * 2f32.powf((m as f32 - 69.0) / 12.0)
}

fn main() -> std::io::Result<()> {
    let mut v = Wind::new(FS, WindKind::UilleannPipes);
    v.set_brightness(0.6); // bright, nasal chanter reed
    v.set_vibrato(5.0, 0.10);

    // Sound the drones: the tonic D in octaves (tenor D3 = 146.83 Hz, plus the
    // baritone and bass octaves below). They drone continuously underneath.
    v.set_drone(146.83, true);

    let mut out = Vec::new();

    // Let the drones speak alone for a beat before the chanter enters.
    for _ in 0..(0.6 * FS) as usize {
        out.push(v.process());
    }

    // A rolling 6/8 jig in D major for the chanter (its D4–A5 range).
    // MIDI: D4=62 E4=64 F#4=66 G4=67 A4=69 B4=71 C#5=73 D5=74 E5=76 F#5=78 A5=81.
    let eighth = 0.2;
    let jig: &[(i32, f32)] = &[
        // Bar 1: D E F# | A F# D  (tonic-rooted opening over the D drones)
        (62, 1.0), (64, 1.0), (66, 1.0), (69, 1.0), (66, 1.0), (62, 1.0),
        // Bar 2: E F# G | F# E D
        (64, 1.0), (66, 1.0), (67, 1.0), (66, 1.0), (64, 1.0), (62, 1.0),
        // Bar 3: F# A D5 | A F# A
        (66, 1.0), (69, 1.0), (74, 1.0), (69, 1.0), (66, 1.0), (69, 1.0),
        // Bar 4: B A G | F# E D
        (71, 1.0), (69, 1.0), (67, 1.0), (66, 1.0), (64, 1.0), (62, 1.5),
        // Bar 5: A B C# | D5 E5 D5
        (69, 1.0), (71, 1.0), (73, 1.0), (74, 1.0), (76, 1.0), (74, 1.0),
        // Bar 6: F#5 E5 D5 | A B A
        (78, 1.0), (76, 1.0), (74, 1.0), (69, 1.0), (71, 1.0), (69, 1.0),
        // Bar 7: G F# E | F# A F#
        (67, 1.0), (66, 1.0), (64, 1.0), (66, 1.0), (69, 1.0), (66, 1.0),
        // Bar 8: E F# D | D (long, settling home on the drone tonic)
        (64, 1.0), (66, 1.0), (62, 1.0), (62, 3.0),
    ];

    for &(m, dur) in jig.iter() {
        let hzf = hz(m);
        v.note_on(hzf, 0.9);
        let n = (dur * eighth * FS) as usize;
        let hold = (n as f32 * 0.85) as usize;
        for j in 0..n {
            let t = j as f32 / n as f32;
            v.set_breath(0.9 + 0.05 * (std::f32::consts::PI * t).sin());
            if j == hold {
                // Chanter notes are tongued/lifted, but the drones keep sounding.
                v.note_off();
            }
            out.push(v.process());
        }
    }

    // Chanter finished; let the drones ring on alone, then fade them.
    v.note_off();
    for _ in 0..(1.0 * FS) as usize {
        out.push(v.process());
    }
    v.set_drone(0.0, false);
    for _ in 0..(0.4 * FS) as usize {
        out.push(v.process());
    }

    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    write_wav(OUT, &out, 0.89 / peak)?;
    println!("Wrote {OUT}: {:.1}s (D-major jig, uilleann chanter + drones)", out.len() as f32 / FS);
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
