//! Render a **sanjo gayageum** phrase: a Korean plucked zither with a warm,
//! singing sustain and deep *nonghyeon* (left-hand vibrato and bends) on the
//! held notes. Pitches are quantized to a Korean-style pentatonic via the Scala
//! quantizer, so a plain semitone sequencer lands on the mode.
//! `cargo run -p plucked-core --example render_gayageum`.

use std::fs::File;
use std::io::{BufWriter, Write};

use plucked_core::Gayageum;
use puget_dsp::{parse_scl, Quantizer};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    // A gayageum sanjo pentatonic (a "gyemyeonjo"-flavoured anhemitonic mode):
    // degrees at roughly the minor pentatonic, expressed as cents so a plain
    // 12-TET sequencer request snaps to the mode.
    let scale = "! gayageum.scl
Gayageum pentatonic (gyemyeonjo-ish)
 5
 300.0
 500.0
 700.0
 1000.0
 2/1
";
    let s = parse_scl(scale).expect("scale");
    let root = 220.0; // A3 — a comfortable gayageum register
    let q = Quantizer::new(s, root);

    let mut g = Gayageum::new(FS);
    g.set_brightness(0.5);
    g.set_sustain(0.85);
    g.set_width(0.35); // one instrument on the lap — modest width

    let mut out_l = Vec::new();
    let mut out_r = Vec::new();

    // A phrase in scale steps (semitone requests, quantized to the pentatonic),
    // each with a length and a nonghyeon depth in cents (0 = plain, big = the
    // trembling, vocal shakes gayageum is loved for).
    let phrase: &[(f32, f32, f32)] = &[
        // (semitone, seconds, vibrato-cents)
        (0.0, 0.55, 0.0),
        (3.0, 0.55, 55.0),  // deep shake on the held note
        (5.0, 0.45, 0.0),
        (7.0, 0.9, 65.0),   // long trembling note
        (5.0, 0.4, 0.0),
        (3.0, 0.7, 45.0),
        (0.0, 1.1, 60.0),   // resolve with a vocal quaver
        (12.0, 0.5, 0.0),
        (10.0, 0.5, 40.0),
        (7.0, 1.4, 70.0),   // long, wide, singing
    ];

    for (k, &(semi, len, vib)) in phrase.iter().enumerate() {
        let hz = q.quantize_volts(semi / 12.0);
        g.pluck(hz, 0.9);
        g.set_vibrato(5.5, vib);
        // On one long note, add a slow upward pitch push (the pressed bend).
        if k == 3 {
            g.set_bend(70.0);
        }
        if k == 0 {
            let et = root * 2f32.powf(semi / 12.0);
            eprintln!("step {semi}: ET {et:.1} Hz -> pentatonic {hz:.1} Hz");
        }
        for _ in 0..(len * FS) as usize {
            let (l, r) = g.process();
            out_l.push(l);
            out_r.push(r);
        }
    }
    // Let the last strings sing out.
    for _ in 0..(4.0 * FS) as usize {
        let (l, r) = g.process();
        out_l.push(l);
        out_r.push(r);
    }

    let peak = out_l
        .iter()
        .chain(out_r.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav("gayageum_demo.wav", &out_l, &out_r, 0.89 / peak)?;
    println!("Wrote gayageum_demo.wav: {:.1}s (sanjo phrase + nonghyeon)", out_l.len() as f32 / FS);
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
