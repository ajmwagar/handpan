//! Render short performances across presets so the voice can be heard.
//! `cargo run -p handpan-core --example render_wav` writes several WAVs:
//!   - handpan_demo.wav         (D Kurd 9, standard handpan)
//!   - handpan_large.wav        (D Kurd 9, large warm handpan)
//!   - tongue_drum.wav          (D Celtic Minor 9, small cut tongue drum)

use std::fs::File;
use std::io::{BufWriter, Write};

use handpan_core::{scale::Scale, Build, Handpan, Size};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    // Lead with the moody/exotic scales.
    render("handpan_hijaz.wav", &Scale::DHijaz9, Build::Handpan, Size::Large)?;
    render("handpan_hijaz_kar.wav", &Scale::DHijazKar9, Build::Handpan, Size::Large)?;
    render("handpan_insen.wav", &Scale::DInsen9, Build::Handpan, Size::Standard)?;
    render("handpan_kurd.wav", &Scale::DKurd9, Build::Handpan, Size::Standard)?;
    render("tongue_drum.wav", &Scale::DCelticMinor9, Build::TongueDrum, Size::Small)?;
    Ok(())
}

fn render(path: &str, scale: &Scale, build: Build, size: Size) -> std::io::Result<()> {
    let n_fields = scale.freqs().len();
    let mut hp = Handpan::from_preset(FS, scale, build, size);

    // A slow, spacious performance so the sustain can sing: the ding, an
    // unhurried ascending run, a gentle descending phrase, and a soft rolling
    // chord to show the sympathetic halo.
    let mut events: Vec<(f32, usize, f32)> = vec![(0.0, 0, 0.9)];
    for (k, field) in (1..n_fields).enumerate() {
        events.push((2.2 + k as f32 * 0.9, field, 0.66));
    }
    let phrase = [n_fields - 1, 5, 4, 2, 1, 3, 1, 0];
    let phrase_start = 2.2 + n_fields as f32 * 0.9 + 1.2;
    for (k, &f) in phrase.iter().enumerate() {
        events.push((phrase_start + k as f32 * 0.8, f.min(n_fields - 1), 0.55 + 0.15 * ((k % 2) as f32)));
    }
    let chord_start = phrase_start + phrase.len() as f32 * 0.8 + 1.2;
    for (k, f) in [0usize, 4, 7, 2].into_iter().enumerate() {
        events.push((chord_start + k as f32 * 0.09, f.min(n_fields - 1), 0.8));
    }

    let total_s = events.iter().map(|e| e.0).fold(0.0, f32::max) + 8.0;
    let total_n = (total_s * FS) as usize;
    events.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let mut ei = 0;
    let mut left = vec![0.0f32; total_n];
    let mut right = vec![0.0f32; total_n];
    for i in 0..total_n {
        let t = i as f32 / FS;
        while ei < events.len() && events[ei].0 <= t {
            let (_, field, vel) = events[ei];
            hp.strike(field, vel);
            ei += 1;
        }
        let (l, r) = hp.process();
        left[i] = l;
        right[i] = r;
    }

    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav_stereo(path, &left, &right, 0.891 / peak)?;
    println!("Wrote {path}: {:.1}s, {} fields, peak {:.3}", total_s, n_fields, peak);
    Ok(())
}

/// Minimal 16-bit PCM stereo WAV writer (no external crates).
fn write_wav_stereo(path: &str, left: &[f32], right: &[f32], gain: f32) -> std::io::Result<()> {
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
