//! Render a short D Kurd handpan performance to `handpan_demo.wav` so the
//! modal voice can be heard. Run with: `cargo run -p handpan-core --example render_wav`.

use std::fs::File;
use std::io::{BufWriter, Write};

use handpan_core::{scale, Handpan};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    let freqs = scale::d_kurd_9();
    let mut hp = Handpan::new(FS, &freqs);

    // Build a little performance as (time_in_seconds, note_index, velocity).
    let mut events: Vec<(f32, usize, f32)> = Vec::new();

    // 1) The ding, left to ring.
    events.push((0.0, 0, 0.95));

    // 2) Ascend the ring notes.
    for (k, note) in (1..freqs.len()).enumerate() {
        events.push((1.2 + k as f32 * 0.45, note, 0.72));
    }

    // 3) A gentle descending melodic phrase.
    let phrase = [8, 6, 5, 3, 2, 4, 1, 0];
    for (k, &n) in phrase.iter().enumerate() {
        events.push((5.6 + k as f32 * 0.38, n, 0.6 + 0.15 * ((k % 2) as f32)));
    }

    // 4) A soft rolling chord to show the sympathetic halo.
    for (k, n) in [0usize, 4, 7, 2].into_iter().enumerate() {
        events.push((9.2 + k as f32 * 0.06, n, 0.8));
    }

    let tail = 6.0;
    let total_s = events.iter().map(|e| e.0).fold(0.0, f32::max) + tail;
    let total_n = (total_s * FS) as usize;

    // Sort events by time and render sample-accurately.
    events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut ei = 0;
    let mut left = vec![0.0f32; total_n];
    let mut right = vec![0.0f32; total_n];
    for i in 0..total_n {
        let t = i as f32 / FS;
        while ei < events.len() && events[ei].0 <= t {
            let (_, note, vel) = events[ei];
            hp.strike(note, vel);
            ei += 1;
        }
        let (l, r) = hp.process();
        left[i] = l;
        right[i] = r;
    }

    // Normalize to -1 dBFS peak.
    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    let norm = 0.891 / peak; // ~ -1 dBFS

    let path = "handpan_demo.wav";
    write_wav_stereo(path, &left, &right, norm)?;
    println!(
        "Wrote {path}: {:.1}s, {} notes, peak {:.3}",
        total_s,
        events.len(),
        peak
    );
    Ok(())
}

/// Minimal 16-bit PCM stereo WAV writer (no external crates).
fn write_wav_stereo(path: &str, left: &[f32], right: &[f32], gain: f32) -> std::io::Result<()> {
    let n = left.len();
    let channels = 2u16;
    let bits = 16u16;
    let sr = FS as u32;
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
            let v = (s * gain).clamp(-1.0, 1.0);
            let q = (v * 32767.0) as i16;
            w.write_all(&q.to_le_bytes())?;
        }
    }
    w.flush()
}
