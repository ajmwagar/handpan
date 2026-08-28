//! Render a lively **Irish fiddle** set: a driving D-major reel followed by a
//! lilting G-major jig, both played over ringing open-string drones and double
//! stops on the shared body — the raw, open trad sound the `Fiddle` voice is
//! built for. `cargo run -p bowed-core --example render_fiddle`.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use bowed_core::{BowedInstrument, StringKind};

const FS: f32 = 48_000.0;

fn midi(n: i32) -> f32 {
    440.0 * 2f32.powf((n as f32 - 69.0) / 12.0)
}

fn main() -> std::io::Result<()> {
    let mut left = Vec::new();
    let mut right = Vec::new();

    let mut v = BowedInstrument::new(FS, StringKind::Fiddle);
    // Bright, open, a little edgy — and dig the bow in toward the bridge.
    v.set_brightness(0.7);
    v.set_bow_position(0.55);
    // A quick, shallow trad vibrato, not the wide operatic sweep.
    v.set_vibrato(6.0, 0.010);

    // Render `secs` of the current instrument state into the buffers, panned
    // gently to taste.
    let render = |v: &mut BowedInstrument, secs: f32, left: &mut Vec<f32>, right: &mut Vec<f32>| {
        let n = (secs * FS) as usize;
        for _ in 0..n {
            let s = v.process();
            left.push(s * 0.70);
            right.push(s * 0.73);
        }
    };

    // --- A driving D-major reel over an open-D / open-A drone --------------
    // Open D (293.66) rings underneath the whole A-part; a running eighth-note
    // melody dances above it on the A and E strings. MIDI: A4=69 .. F#5=78.
    let eighth = 0.15; // a brisk reel tempo
    v.note_on(293.66, 0.55, 0.45); // open D drone (bottom of the double stops)

    // (melody MIDI note, on the strong beat also sound the open A as a stop?)
    let reel: &[(i32, bool)] = &[
        // bar 1
        (69, true), (74, false), (73, false), (74, false),
        (76, true), (74, false), (73, false), (71, false),
        // bar 2
        (69, true), (69, false), (76, false), (74, false),
        (73, true), (74, false), (76, false), (78, false),
        // bar 3
        (81, true), (78, false), (76, false), (74, false),
        (73, true), (74, false), (76, false), (73, false),
        // bar 4 — turn back to the tonic
        (74, true), (69, false), (66, false), (69, false),
        (74, true), (76, false), (74, false), (74, false),
    ];
    for &(m, strong) in reel {
        v.note_on(midi(m), 0.78, 0.55);
        if strong {
            v.note_on(440.0, 0.6, 0.45); // open-A double stop on the beat
        }
        render(&mut v, eighth, &mut left, &mut right);
    }
    v.note_off_all();
    render(&mut v, 0.7, &mut left, &mut right); // let the drone ring out

    // --- A lilting G-major jig over an open-G / open-D drone --------------
    // 6/8 feel: groups of three. Open G (196) drones under the tune.
    let jig8 = 0.16;
    v.note_on(196.0, 0.55, 0.45); // open G drone
    let jig: &[(i32, bool)] = &[
        // |: G A B  |  d B G |
        (67, true), (69, false), (71, false),
        (74, true), (71, false), (67, false),
        // | A B c |  A F# D |
        (69, true), (71, false), (72, false),
        (69, true), (66, false), (62, false),
        // | G A B |  d g d |
        (67, true), (69, false), (71, false),
        (74, true), (79, false), (74, false),
        // | B A G |  A B G :|
        (71, true), (69, false), (67, false),
        (69, true), (71, false), (67, false),
    ];
    for &(m, strong) in jig {
        v.note_on(midi(m), 0.76, 0.55);
        if strong {
            v.note_on(293.66, 0.55, 0.45); // open-D double stop on the dotted beat
        }
        render(&mut v, jig8, &mut left, &mut right);
    }
    v.note_off_all();
    render(&mut v, 1.6, &mut left, &mut right); // final ring-out

    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |mx, &x| mx.max(x.abs()))
        .max(1e-9);
    write_wav("fiddle_demo.wav", &left, &right, 0.89 / peak)?;
    println!("Wrote fiddle_demo.wav: {:.1}s, peak {peak:.3} (D reel + G jig)", left.len() as f32 / FS);
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
