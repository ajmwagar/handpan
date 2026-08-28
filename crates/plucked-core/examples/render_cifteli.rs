//! Render a çifteli: a droning G with a melody line quantized to **maqam Rast**
//! (a microtonal scale with neutral 3rd/7th) via the Scala quantizer — the
//! microtonal, drone+melody character of the instrument.
//! `cargo run -p plucked-core --example render_cifteli`.

use std::fs::File;
use std::io::{BufWriter, Write};

use plucked_core::Cifteli;
use puget_dsp::{parse_scl, Quantizer};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    // Maqam Rast on the root (neutral 3rd ≈ 350c, neutral 7th ≈ 1050c) — the
    // microtones a 12-note quantizer could never give you.
    let rast = "! rast.scl
Maqam Rast (approx)
 7
 200.0
 350.0
 500.0
 700.0
 900.0
 1050.0
 2/1
";
    let scale = parse_scl(rast).expect("rast");
    let root = 196.0; // G3
    let q = Quantizer::new(scale, root);

    let mut c = Cifteli::new(FS);
    c.set_drone_hz(root);
    c.set_brightness(0.62);
    c.set_sustain(0.75);

    let mut out_l = Vec::new();
    let mut out_r = Vec::new();

    // A melody as scale steps above the root (in "12-TET-ish" semitone requests
    // that the quantizer snaps to the nearest Rast degree — so even a plain
    // sequencer lands on the maqam).
    let melody: &[(f32, f32)] = &[
        (0.0, 0.4), (2.0, 0.3), (4.0, 0.3), (5.0, 0.4), (7.0, 0.4), (5.0, 0.3),
        (4.0, 0.3), (2.0, 0.4), (0.0, 0.5), (7.0, 0.3), (9.0, 0.3), (11.0, 0.4),
        (12.0, 0.6), (11.0, 0.3), (9.0, 0.3), (7.0, 0.5), (4.0, 0.4), (0.0, 1.0),
    ];

    // Pluck the drone up front (it rings under everything).
    c.pluck_drone(0.8);
    let mut drone_timer = 0usize;
    let redrone = (2.2 * FS) as usize;

    for (k, &(semi, len)) in melody.iter().enumerate() {
        // A sequencer step in semitones from the root → V/Oct volts → quantized.
        let volts = semi / 12.0;
        let hz = q.quantize_volts(volts);
        c.pluck_melody(hz, 0.9);
        if k == 0 {
            // print what the quantizer chose vs the equal-tempered request
            let et = root * 2f32.powf(volts);
            eprintln!("step {semi}: ET {et:.1} Hz → Rast {hz:.1} Hz");
        }
        let n = (len * FS) as usize;
        for _ in 0..n {
            if drone_timer >= redrone {
                c.pluck_drone(0.55);
                drone_timer = 0;
            }
            drone_timer += 1;
            let (l, r) = c.process();
            out_l.push(l);
            out_r.push(r);
        }
    }
    // ring out
    for _ in 0..(3.0 * FS) as usize {
        let (l, r) = c.process();
        out_l.push(l);
        out_r.push(r);
    }

    let peak = out_l
        .iter()
        .chain(out_r.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav("cifteli_demo.wav", &out_l, &out_r, 0.89 / peak)?;
    println!("Wrote cifteli_demo.wav: {:.1}s (drone + Rast melody)", out_l.len() as f32 / FS);
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
