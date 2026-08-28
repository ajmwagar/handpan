//! Render a **santur** phrase in **Dastgah-e Shur**: the Persian hammered
//! dulcimer, struck-string shimmer with fast two-mallet tremolo rolls, tuned to
//! a real dastgah via the Scala quantizer + the bundled `.scl` pack.
//! `cargo run -p plucked-core --example render_santur`.

use std::fs::File;
use std::io::{BufWriter, Write};

use plucked_core::Santur;
use puget_dsp::{parse_scl, Quantizer};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    // Load Dastgah-e Shur from the tuning pack (neutral 6th ≈ 783 cents — the
    // koron that makes it unmistakably Persian).
    let shur = parse_scl(include_str!("../../../scales/dastgah/shur.scl")).expect("shur");
    let root = 293.66; // D4 tonic (shahed)
    let q = Quantizer::new(shur, root);

    let mut s = Santur::new(FS);
    s.set_brightness(0.7);
    s.set_sustain(0.85);
    s.set_shimmer(6.0); // a wet, beating unison
    s.set_width(0.45);

    let mut out_l = Vec::new();
    let mut out_r = Vec::new();

    // A phrase in scale steps (semitone requests, quantized to Shur), each with
    // a length and a `roll` flag: rolled notes get a fast mallet tremolo, the
    // santur's signature; short notes are single strikes.
    let phrase: &[(f32, f32, bool)] = &[
        (0.0, 0.30, false), (2.0, 0.30, false), (3.0, 0.30, false), (5.0, 0.30, false),
        (7.0, 0.9, true),   // rolled
        (5.0, 0.30, false), (3.0, 0.30, false),
        (2.0, 0.7, true),   // rolled
        (0.0, 0.30, false), (3.0, 0.30, false), (5.0, 0.30, false), (7.0, 0.30, false),
        (8.0, 1.0, true),   // the neutral 6th (koron), rolled
        (7.0, 0.30, false), (5.0, 0.30, false), (3.0, 0.30, false),
        (0.0, 1.4, true),   // resolve on the tonic, long roll
    ];

    let roll_hz = 13.0; // mallet tremolo rate
    let roll_period = (FS / roll_hz) as usize;

    for &(semi, len, roll) in phrase {
        let hz = q.quantize_volts(semi / 12.0);
        let n = (len * FS) as usize;
        let mut since = roll_period; // strike immediately
        for i in 0..n {
            if i == 0 || (roll && since >= roll_period) {
                // Alternate a hair of velocity like two hands would.
                let vel = if (i / roll_period) % 2 == 0 { 0.9 } else { 0.78 };
                s.strike(hz, vel);
                since = 0;
            }
            since += 1;
            let (l, r) = s.process();
            out_l.push(l);
            out_r.push(r);
        }
    }
    // Shimmer out.
    for _ in 0..(3.5 * FS) as usize {
        let (l, r) = s.process();
        out_l.push(l);
        out_r.push(r);
    }

    let peak = out_l
        .iter()
        .chain(out_r.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav("santur_demo.wav", &out_l, &out_r, 0.89 / peak)?;
    println!("Wrote santur_demo.wav: {:.1}s (Dastgah-e Shur)", out_l.len() as f32 / FS);
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
