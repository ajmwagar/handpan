//! Render the "handpan module that plays itself" — the generative sequencer
//! driving the voice, as it would in a Eurorack patch.
//! `cargo run -p handpan-core --example render_module`.

use std::fs::File;
use std::io::{BufWriter, Write};

use handpan_core::{scale::Scale, Build, HandpanInstrument, PlayMode, Size};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    // Slow ambient wander on D Hijaz, Large — one clock, plays itself.
    let mut wander = HandpanInstrument::new(FS, &Scale::DHijaz9, Build::Handpan, Size::Large);
    wander.set_mode(PlayMode::Wander);
    wander.set_feel(0.22, 0.18);
    wander.set_air(0.22);
    render("module_wander_hijaz.wav", &mut wander, 0.7, 0.62)?; // ~0.62s between clocks

    // Euclidean pulse on D Insen — more rhythmic.
    let mut euclid = HandpanInstrument::new(FS, &Scale::DInsen9, Build::Handpan, Size::Standard);
    euclid.set_mode(PlayMode::Euclid);
    euclid.set_euclid(8, 5, 0);
    euclid.set_feel(0.15, 0.0);
    euclid.set_air(0.18);
    render("module_euclid_insen.wav", &mut euclid, 0.75, 0.3)?; // faster clock

    Ok(())
}

fn render(path: &str, inst: &mut HandpanInstrument, vel: f32, clock_s: f32) -> std::io::Result<()> {
    let total_s = 26.0;
    let total_n = (total_s * FS) as usize;
    let clock_n = (clock_s * FS) as usize;
    let mut left = vec![0.0f32; total_n];
    let mut right = vec![0.0f32; total_n];
    for i in 0..total_n {
        if i % clock_n == 0 && i < total_n - (4.0 * FS) as usize {
            inst.clock(vel); // stop clocking for the last 4s so it rings out
        }
        let (l, r) = inst.process();
        left[i] = l;
        right[i] = r;
    }
    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav_stereo(path, &left, &right, 0.891 / peak)?;
    println!("Wrote {path}: {total_s:.0}s, clock {clock_s:.2}s, peak {peak:.3}");
    Ok(())
}

fn write_wav_stereo(path: &str, left: &[f32], right: &[f32], gain: f32) -> std::io::Result<()> {
    let n = left.len();
    let (channels, bits, sr) = (2u16, 16u16, FS as u32);
    let block_align = channels * bits / 8;
    let data_len = (n as u32) * block_align as u32;
    let mut w = BufWriter::new(File::create(path)?);
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sr.to_le_bytes())?;
    w.write_all(&(sr * block_align as u32).to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&bits.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    for i in 0..n {
        for &s in &[left[i], right[i]] {
            let q = ((s * gain).clamp(-1.0, 1.0) * 32767.0) as i16;
            w.write_all(&q.to_le_bytes())?;
        }
    }
    w.flush()
}
