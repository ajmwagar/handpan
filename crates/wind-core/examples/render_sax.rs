//! A short alto-sax phrase — to hear the new attack chiff and phrasing in
//! context. `cargo run -p wind-core --example render_sax`.
use std::fs::File;
use std::io::{BufWriter, Write};
use wind_core::{Wind, WindKind};
const FS: f32 = 48_000.0;
fn midi(n: i32) -> f32 {
    440.0 * 2f32.powf((n as f32 - 69.0) / 12.0)
}
fn main() -> std::io::Result<()> {
    let mut out = Vec::new();
    let mut v = Wind::new(FS, WindKind::Saxophone);
    v.set_vibrato(5.0, 0.22);
    v.set_brightness(0.55);
    // A bluesy alto line with separate articulated notes (each gets a chiff).
    let phrase: &[(i32, f32)] = &[
        (58, 0.35), (61, 0.28), (63, 0.28), (65, 0.5), (63, 0.3), (61, 0.3),
        (58, 0.45), (56, 0.3), (58, 0.7), (61, 0.35), (63, 0.9), (58, 1.3),
    ];
    for (i, &(m, len)) in phrase.iter().enumerate() {
        let note_n = (len * FS) as usize;
        let hold = (note_n as f32 * 0.82) as usize;
        v.note_on(midi(m), 0.85);
        for j in 0..note_n {
            let t = j as f32 / note_n as f32;
            v.set_breath(0.85 * (0.8 + 0.2 * (std::f32::consts::PI * t).sin()));
            if j == hold && i != phrase.len() - 1 {
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
    let mut w = BufWriter::new(File::create("sax_phrase.wav")?);
    let (ch, bits, sr) = (1u16, 16u16, FS as u32);
    let ba = ch * bits / 8;
    let dl = (out.len() as u32) * ba as u32;
    w.write_all(b"RIFF")?; w.write_all(&(36 + dl).to_le_bytes())?; w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?; w.write_all(&16u32.to_le_bytes())?; w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&ch.to_le_bytes())?; w.write_all(&sr.to_le_bytes())?;
    w.write_all(&(sr * ba as u32).to_le_bytes())?; w.write_all(&ba.to_le_bytes())?;
    w.write_all(&bits.to_le_bytes())?; w.write_all(b"data")?; w.write_all(&dl.to_le_bytes())?;
    let g = 0.89 / peak;
    for &s in &out {
        w.write_all(&(((s * g).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    w.flush()?;
    println!("Wrote sax_phrase.wav: {:.1}s", out.len() as f32 / FS);
    Ok(())
}
