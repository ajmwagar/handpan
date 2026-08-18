//! Single sustained note for spectral tuning against reference recordings.
//! `cargo run -q -p wind-core --example probe -- <midi> <brightness>`
use std::fs::File;
use std::io::{BufWriter, Write};
use wind_core::{Wind, WindKind};
const FS: f32 = 44_100.0;
fn main() -> std::io::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let midi: i32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(62);
    let bright: f32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.5);
    let breath: f32 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.9);
    let kind = match a.get(4).map(|s| s.as_str()) {
        Some("sax") => WindKind::Saxophone,
        Some("trumpet") => WindKind::Trumpet,
        _ => WindKind::Clarinet,
    };
    let f0 = 440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0);
    let mut v = Wind::new(FS, kind);
    v.set_brightness(bright);
    v.note_on(f0, breath);
    let n = (1.5 * FS) as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(v.process());
    }
    let peak = out.iter().fold(0.0f32, |m, &x| m.max(x.abs())).max(1e-9);
    let g = 0.9 / peak;
    let mut w = BufWriter::new(File::create("probe.wav")?);
    let (ch, bits, sr) = (1u16, 16u16, FS as u32);
    let ba = ch * bits / 8;
    let dl = (n as u32) * ba as u32;
    w.write_all(b"RIFF")?; w.write_all(&(36 + dl).to_le_bytes())?; w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?; w.write_all(&16u32.to_le_bytes())?; w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&ch.to_le_bytes())?; w.write_all(&sr.to_le_bytes())?;
    w.write_all(&(sr * ba as u32).to_le_bytes())?; w.write_all(&ba.to_le_bytes())?;
    w.write_all(&bits.to_le_bytes())?; w.write_all(b"data")?; w.write_all(&dl.to_le_bytes())?;
    for &s in &out {
        w.write_all(&(((s * g).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    w.flush()?;
    println!("probe.wav midi={midi} f0={f0:.1} bright={bright}");
    Ok(())
}
