//! Audible demo of the shared [`puget_dsp::Reverb`]. Synthesizes a short
//! decaying-sine "pluck" plus a bright impulse, runs them through the reverb,
//! and writes dry-vs-wet stereo WAVs to the scratchpad so the space is audible:
//!   - reverb_dry.wav        (source, no reverb)
//!   - reverb_wet_room.wav   (medium room, ~1.8 s)
//!   - reverb_wet_hall.wav   (large hall, ~5 s, darker)
//!
//! Run: `cargo run -p puget-dsp --example reverb_demo`
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::fs::File;
use std::io::{BufWriter, Write};

use puget_dsp::Reverb;

const FS: f32 = 48_000.0;
const OUT_DIR: &str =
    "/tmp/claude-0/-home-user-handpan/6928443b-c2eb-5baf-9092-20735a10aa33/scratchpad";

fn main() -> std::io::Result<()> {
    // --- Source: a few decaying-sine plucks, then a bright transient click. ---
    let secs = 8.0;
    let n = (secs * FS) as usize;
    let mut src = vec![0.0f32; n];

    let notes = [(0.0f32, 220.0f32), (0.6, 277.18), (1.2, 329.63), (1.8, 440.0)];
    for &(t0, freq) in &notes {
        let start = (t0 * FS) as usize;
        for i in 0..(FS as usize) {
            // 1 s per note, exponential decay.
            let idx = start + i;
            if idx >= n {
                break;
            }
            let t = i as f32 / FS;
            let env = (-t * 6.0).exp();
            let s = (2.0 * std::f32::consts::PI * freq * t).sin();
            src[idx] += 0.6 * env * s;
        }
    }
    // A dry click at 3.2 s so the reverb's early reflections are obvious.
    let click = (3.2 * FS) as usize;
    if click < n {
        src[click] += 0.9;
    }

    // Mono source fed as (x, x).
    write_wav(&format!("{OUT_DIR}/reverb_dry.wav"), &src, &src)?;

    // Medium room.
    let mut room = Reverb::new(FS);
    room.set_decay(1.8);
    room.set_damp(0.3);
    room.set_mix(0.35);
    room.set_width(1.0);
    let (rl, rr) = render(&mut room, &src);
    write_wav(&format!("{OUT_DIR}/reverb_wet_room.wav"), &rl, &rr)?;

    // Large, darker hall.
    let mut hall = Reverb::new(FS);
    hall.set_size(0.85);
    hall.set_damp(0.6);
    hall.set_mix(0.45);
    hall.set_width(1.0);
    let (hl, hr) = render(&mut hall, &src);
    write_wav(&format!("{OUT_DIR}/reverb_wet_hall.wav"), &hl, &hr)?;

    println!("Wrote reverb_dry / reverb_wet_room / reverb_wet_hall to {OUT_DIR}");
    println!("  room decay = {:.2}s, hall decay = {:.2}s", room.decay(), hall.decay());
    Ok(())
}

fn render(rv: &mut Reverb, src: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut l = vec![0.0f32; src.len()];
    let mut r = vec![0.0f32; src.len()];
    for i in 0..src.len() {
        let (ol, or) = rv.process(src[i], src[i]);
        l[i] = ol;
        r[i] = or;
    }
    (l, r)
}

/// Minimal 16-bit PCM stereo WAV writer, normalized to −1 dBFS (no crates).
fn write_wav(path: &str, left: &[f32], right: &[f32]) -> std::io::Result<()> {
    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    let gain = 0.891 / peak;

    let n = left.len();
    let (channels, bits, sr) = (2u16, 16u16, FS as u32);
    let block_align = channels * bits / 8;
    let byte_rate = sr * block_align as u32;
    let data_len = (n as u32) * block_align as u32;

    let mut w = BufWriter::new(File::create(path)?);
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVE")?;
    w.write_all(b"fmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&channels.to_le_bytes())?;
    w.write_all(&sr.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
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
