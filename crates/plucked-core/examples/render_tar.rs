//! Render a **tar** phrase in **Dastgah-e Chahargah**: the Persian long-neck
//! lute, doubled-course strings over a bright skin-membrane top, with the
//! characteristic plectrum *riz* (rapid tremolo) on the held notes — tuned to a
//! real dastgah via the Scala quantizer + the bundled `.scl` pack.
//! `cargo run -p plucked-core --example render_tar`.

use std::fs::File;
use std::io::{BufWriter, Write};

use plucked_core::Tar;
use puget_dsp::{parse_scl, Quantizer};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    let scale = parse_scl(include_str!("../../../scales/dastgah/chahargah.scl")).expect("chahargah");
    let root = 261.63; // C4 tonic
    let q = Quantizer::new(scale, root);

    let mut t = Tar::new(FS);
    t.set_brightness(0.6);
    t.set_sustain(0.8);
    t.set_chorus(4.5);
    t.set_width(0.4);

    let mut out_l = Vec::new();
    let mut out_r = Vec::new();

    // (scale-step semitones, seconds, riz?) — riz notes get a rapid tremolo.
    let phrase: &[(f32, f32, bool)] = &[
        (0.0, 0.32, false), (1.0, 0.32, false), (3.0, 0.32, false), (4.0, 0.32, false),
        (5.0, 0.9, true),   // held, tremolo
        (4.0, 0.32, false), (3.0, 0.32, false),
        (1.0, 0.75, true),
        (0.0, 0.32, false), (3.0, 0.32, false), (5.0, 0.32, false), (7.0, 0.32, false),
        (8.0, 1.0, true),   // upper held note, riz
        (7.0, 0.32, false), (5.0, 0.32, false), (4.0, 0.32, false),
        (0.0, 1.5, true),   // resolve on the tonic with a long riz
    ];

    let riz_hz = 16.0;
    let riz_period = (FS / riz_hz) as usize;

    for &(semi, len, riz) in phrase {
        let hz = q.quantize_volts(semi / 12.0);
        let n = (len * FS) as usize;
        let mut since = riz_period;
        for i in 0..n {
            if i == 0 || (riz && since >= riz_period) {
                let vel = if (i / riz_period) % 2 == 0 { 0.85 } else { 0.7 };
                t.pluck(hz, vel);
                since = 0;
            }
            since += 1;
            let (l, r) = t.process();
            out_l.push(l);
            out_r.push(r);
        }
    }
    for _ in 0..(3.0 * FS) as usize {
        let (l, r) = t.process();
        out_l.push(l);
        out_r.push(r);
    }

    let peak = out_l
        .iter()
        .chain(out_r.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav("tar_demo.wav", &out_l, &out_r, 0.89 / peak)?;
    println!("Wrote tar_demo.wav: {:.1}s (Dastgah-e Chahargah)", out_l.len() as f32 / FS);
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
