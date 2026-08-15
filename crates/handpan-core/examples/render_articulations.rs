//! Showcase the articulations: open vs mute vs slap, the gu bass hit, a
//! center→edge position sweep, a palm-mute (pressure) swell, and A=432 tuning.
//! `cargo run -p handpan-core --example render_articulations`.

use std::fs::File;
use std::io::{BufWriter, Write};

use handpan_core::{scale::Scale, Artic, Build, Handpan, Size, VoiceProfile};

const FS: f32 = 48_000.0;

enum Act {
    Strike(usize, f32, Artic, f32),
    Gu(f32),
    Damp(f32),
}

fn main() -> std::io::Result<()> {
    let mut hp = Handpan::from_preset(FS, &Scale::DHijaz9, Build::Handpan, Size::Large);
    hp.set_air(0.18);

    let mut acts: Vec<(f32, Act)> = Vec::new();
    let mut t = 0.3f32;

    // 1) Open tones up the scale.
    for f in [0usize, 2, 4, 6] {
        acts.push((t, Act::Strike(f, 0.85, Artic::Open, 0.4)));
        t += 0.6;
    }
    t += 0.5;
    // 2) The same notes muted (short, thuddy).
    for f in [0usize, 2, 4, 6] {
        acts.push((t, Act::Strike(f, 0.85, Artic::Mute, 0.4)));
        t += 0.35;
    }
    t += 0.5;
    // 3) Slaps (bright, percussive).
    for f in [1usize, 3, 5, 7] {
        acts.push((t, Act::Strike(f, 0.9, Artic::Slap, 0.7)));
        t += 0.3;
    }
    t += 0.5;
    // 4) Gu bass hits under an open melody.
    for (k, f) in [4usize, 6, 3, 5].into_iter().enumerate() {
        if k % 2 == 0 {
            acts.push((t, Act::Gu(0.9)));
        }
        acts.push((t + 0.15, Act::Strike(f, 0.75, Artic::Open, 0.35)));
        t += 0.7;
    }
    t += 0.6;
    // 5) Position sweep: same note, center → edge.
    for k in 0..6 {
        acts.push((t, Act::Strike(2, 0.8, Artic::Open, k as f32 / 5.0)));
        t += 0.45;
    }
    t += 0.5;
    // 6) Open chord, then a palm-mute pressure swell chokes it.
    for f in [0usize, 4, 7] {
        acts.push((t, Act::Strike(f, 0.85, Artic::Open, 0.4)));
    }
    for k in 1..=20 {
        acts.push((t + k as f32 * 0.08, Act::Damp(k as f32 / 20.0)));
    }
    t += 3.5;

    render(&mut hp, acts, t + 2.5, "articulations.wav")?;

    // 7) The opening phrase at A=432 for comparison.
    let mut prof = VoiceProfile::preset(Build::Handpan, Size::Large);
    prof.tune_cents = -31.8; // A=432
    prof.air = 0.18;
    let mut hp432 = Handpan::with_profile(FS, &Scale::DHijaz9.freqs(), &prof);
    let mut a432: Vec<(f32, Act)> = Vec::new();
    let mut tt = 0.3f32;
    for f in [0usize, 2, 4, 6] {
        a432.push((tt, Act::Strike(f, 0.85, Artic::Open, 0.4)));
        tt += 0.6;
    }
    render(&mut hp432, a432, tt + 3.0, "articulations_432.wav")?;
    Ok(())
}

fn render(hp: &mut Handpan, mut acts: Vec<(f32, Act)>, total_s: f32, path: &str) -> std::io::Result<()> {
    acts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let total_n = (total_s * FS) as usize;
    let mut left = vec![0.0f32; total_n];
    let mut right = vec![0.0f32; total_n];
    let mut ai = 0usize;
    for i in 0..total_n {
        let t = i as f32 / FS;
        while ai < acts.len() && acts[ai].0 <= t {
            match &acts[ai].1 {
                Act::Strike(f, v, a, p) => hp.strike_artic(*f, *v, *a, *p),
                Act::Gu(v) => hp.strike_gu(*v),
                Act::Damp(x) => hp.set_damp(*x),
            }
            ai += 1;
        }
        let (l, r) = hp.process();
        left[i] = l;
        right[i] = r;
    }
    // set_damp is sticky; leave it (the phrase ends muted intentionally).
    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()))
        .max(1e-9);
    write_wav_stereo(path, &left, &right, 0.891 / peak)?;
    println!("Wrote {path}: {total_s:.1}s, {} events, peak {peak:.3}", acts.len());
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
