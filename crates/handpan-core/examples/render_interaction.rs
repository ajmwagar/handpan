//! A/B the cross-note interaction: the same dyads with the shared-shell
//! nonlinearity off (notes sum independently) vs on (they intermodulate into
//! combination tones). `cargo run -p handpan-core --example render_interaction`.

use std::fs::File;
use std::io::{BufWriter, Write};

use handpan_core::{scale::Scale, Build, Handpan, Size};

const FS: f32 = 48_000.0;

fn main() -> std::io::Result<()> {
    render("interaction_off.wav", 0.0)?;
    render("interaction_on.wav", 0.22)?;
    Ok(())
}

fn render(path: &str, shell: f32) -> std::io::Result<()> {
    let mut hp = Handpan::from_preset(FS, &Scale::DHijaz9, Build::Handpan, Size::Large);
    hp.set_shell_nonlin(shell);

    // Dyads struck together (two fields, same instant), hard, spaced out to
    // ring. Pairs chosen to be harmonically close so combination tones show.
    let dyads = [(0usize, 4usize), (0, 6), (1, 5), (2, 7), (0, 3)];
    let mut events: Vec<(f32, usize, f32)> = Vec::new();
    for (k, &(a, b)) in dyads.iter().enumerate() {
        let t = 0.4 + k as f32 * 3.2;
        events.push((t, a, 0.95));
        events.push((t, b, 0.9));
    }

    let total_s = events.iter().map(|e| e.0).fold(0.0, f32::max) + 6.0;
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
    // Normalize both files with the SAME gain so the A/B is level-matched.
    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav_stereo(path, &left, &right, 0.7 / peak)?;
    println!("Wrote {path}: shell_nonlin {shell:.2}, {} dyads, peak {peak:.3}", dyads.len());
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
