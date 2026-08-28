//! Modal analyzer: turn note recordings into a `handpan-core` timbre.
//!
//! For each recording it finds the spectral partials (parabolic-interpolated
//! peak frequency + amplitude) and estimates each partial's T60 decay by
//! tracking its STFT bin over time. Partials are expressed relative to the
//! fundamental (ratio, gain, decay) and aggregated across notes into a single
//! `&[ModeSpec]` printed as ready-to-paste Rust.
//!
//! Usage:
//!   handpan-analyzer <file_or_dir> [<file_or_dir> ...]
//!
//! Recording filenames may encode the note (e.g. `s8-D4.mp3`) to seed the
//! fundamental; otherwise the lowest strong partial is used.

use std::path::{Path, PathBuf};

use rustfft::{num_complex::Complex, FftPlanner};

const TARGET_HARMONICS: [f32; 4] = [1.0, 2.0, 3.0, 4.0];

struct Partial {
    freq: f32,
    ratio: f32,
    gain: f32, // linear, relative to fundamental
    decay: f32, // T60 ratio, relative to fundamental
    t60: f32,   // absolute seconds
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: handpan-analyzer <file_or_dir> ...");
        std::process::exit(2);
    }

    let mut files = Vec::new();
    for a in &args {
        let p = PathBuf::from(a);
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(&p) {
                for e in rd.flatten() {
                    let path = e.path();
                    if path.extension().map(|x| x == "mp3").unwrap_or(false) {
                        files.push(path);
                    }
                }
            }
        } else {
            files.push(p);
        }
    }
    files.sort();

    // Collect partials from every note, bucketed by nearest target harmonic.
    let mut harm_gain: Vec<Vec<f32>> = vec![Vec::new(); TARGET_HARMONICS.len()];
    let mut harm_decay: Vec<Vec<f32>> = vec![Vec::new(); TARGET_HARMONICS.len()];
    let mut harm_ratio: Vec<Vec<f32>> = vec![Vec::new(); TARGET_HARMONICS.len()];
    // Inharmonic shimmer partials (ratio > 4.4).
    let mut shimmer: Vec<Partial> = Vec::new();

    for path in &files {
        let (samples, fs) = match decode_mono(path) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("skip {}: {e}", path.display());
                continue;
            }
        };
        let f0_hint = note_from_filename(path);
        let partials = analyze(&samples, fs, f0_hint);
        if partials.is_empty() {
            eprintln!("skip {}: no partials", path.display());
            continue;
        }
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
        println!(
            "# {name}: f0={:.1}Hz  {} partials",
            partials[0].freq,
            partials.len()
        );
        for p in &partials {
            println!(
                "#   ratio {:.4}  gain {:.3}  t60 {:.2}s  (decay×{:.2})",
                p.ratio, p.gain, p.t60, p.decay
            );
            if p.ratio > 4.4 {
                shimmer.push(Partial { ..*p });
                continue;
            }
            // Bucket into the nearest tuned harmonic if within 6%.
            let mut best = 0usize;
            let mut bd = f32::INFINITY;
            for (i, h) in TARGET_HARMONICS.iter().enumerate() {
                let d = (p.ratio - h).abs() / h;
                if d < bd {
                    bd = d;
                    best = i;
                }
            }
            if bd < 0.06 {
                harm_ratio[best].push(p.ratio);
                harm_gain[best].push(p.gain);
                harm_decay[best].push(p.decay);
            }
        }
    }

    println!("\n// --- Measured handpan timbre (from {} notes) ---", files.len());
    println!("pub const MEASURED_HANDPAN_TIMBRE: &[ModeSpec] = &[");
    for (i, h) in TARGET_HARMONICS.iter().enumerate() {
        if harm_gain[i].is_empty() {
            continue;
        }
        let ratio = median(&mut harm_ratio[i]);
        let gain = median(&mut harm_gain[i]);
        let decay = median(&mut harm_decay[i]);
        println!(
            "    ModeSpec {{ ratio: {:.4}, gain: {:.3}, decay: {:.3} }}, // ~{}x  (n={})",
            ratio,
            gain,
            decay,
            *h as u32,
            harm_gain[i].len()
        );
    }
    // Two representative shimmer modes from the strongest inharmonic partials.
    shimmer.sort_by(|a, b| b.gain.partial_cmp(&a.gain).unwrap());
    for p in shimmer.iter().take(2) {
        println!(
            "    ModeSpec {{ ratio: {:.4}, gain: {:.3}, decay: {:.3} }}, // shimmer",
            p.ratio, p.gain.min(0.2), p.decay
        );
    }
    println!("];");
}

/// Decode an mp3 to mono f32 samples plus its sample rate.
fn decode_mono(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let (header, samples) = puremp3::read_mp3(&data[..]).map_err(|e| format!("{e:?}"))?;
    let fs = header.sample_rate.hz();
    let mono: Vec<f32> = samples.map(|(l, r)| 0.5 * (l + r)).collect();
    Ok((mono, fs))
}

/// Pull a fundamental-frequency hint out of a filename like `s8-D4`.
fn note_from_filename(path: &Path) -> Option<f32> {
    let stem = path.file_stem()?.to_str()?;
    let tok = stem.rsplit('-').next()?; // "D4", "Bb3", "d2"
    note_name_to_freq(tok)
}

fn note_name_to_freq(tok: &str) -> Option<f32> {
    let b = tok.as_bytes();
    if b.is_empty() {
        return None;
    }
    let letter = (b[0] as char).to_ascii_uppercase();
    let mut semis = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut i = 1;
    while i < b.len() && (b[i] == b'#' || b[i] == b'b' || b[i] == b'S' || b[i] == b's') {
        if b[i] == b'#' || b[i] == b'S' || b[i] == b's' {
            semis += 1;
        } else {
            semis -= 1;
        }
        i += 1;
    }
    let octave: i32 = tok[i..].parse().ok()?;
    let midi = 12 * (octave + 1) + semis;
    Some(440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0))
}

/// Extract partials (freq, relative gain, relative + absolute decay).
fn analyze(samples: &[f32], fs: u32, f0_hint: Option<f32>) -> Vec<Partial> {
    let fsf = fs as f32;
    // Attack = loudest sample; analyze from just after it.
    let attack = samples
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let start = (attack + (0.02 * fsf) as usize).min(samples.len().saturating_sub(1));
    let sig = &samples[start..];
    if sig.len() < 8192 {
        return Vec::new();
    }

    // High-resolution spectrum for precise partial frequencies.
    let nfft = 1usize << 16;
    let win_len = sig.len().min(nfft);
    let mut buf = vec![Complex::new(0.0f32, 0.0); nfft];
    for i in 0..win_len {
        let w = hann(i, win_len);
        buf[i] = Complex::new(sig[i] * w, 0.0);
    }
    let mut planner = FftPlanner::new();
    planner.plan_fft_forward(nfft).process(&mut buf);
    let half = nfft / 2;
    let mag: Vec<f32> = buf[..half].iter().map(|c| c.norm()).collect();

    let max_mag = mag.iter().cloned().fold(0.0f32, f32::max).max(1e-12);
    let thresh = max_mag * 10f32.powf(-50.0 / 20.0); // 50 dB below peak
    let bin_hz = fsf / nfft as f32;

    // Local maxima with parabolic interpolation.
    let mut peaks: Vec<(f32, f32)> = Vec::new(); // (freq, amp)
    for n in 2..half - 2 {
        if mag[n] > thresh && mag[n] >= mag[n - 1] && mag[n] > mag[n + 1] {
            let (dl, dc, dr) = (
                (mag[n - 1] + 1e-12).ln(),
                (mag[n] + 1e-12).ln(),
                (mag[n + 1] + 1e-12).ln(),
            );
            let denom = dl - 2.0 * dc + dr;
            let delta = if denom.abs() > 1e-9 {
                0.5 * (dl - dr) / denom
            } else {
                0.0
            };
            let freq = (n as f32 + delta) * bin_hz;
            let amp = (dc - 0.25 * (dl - dr) * delta).exp();
            if freq >= 40.0 && freq <= fsf * 0.45 {
                peaks.push((freq, amp));
            }
        }
    }
    if peaks.is_empty() {
        return Vec::new();
    }
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    peaks.truncate(14);

    // Fundamental: nearest peak to the hint, else the lowest strong peak.
    let f_fund = if let Some(h) = f0_hint {
        peaks
            .iter()
            .min_by(|a, b| (a.0 - h).abs().partial_cmp(&(b.0 - h).abs()).unwrap())
            .unwrap()
            .0
    } else {
        peaks.iter().map(|p| p.0).fold(f32::INFINITY, f32::min)
    };

    // Decay per partial via STFT bin tracking.
    let t60_fund = estimate_t60(sig, fs, f_fund);
    let mut partials: Vec<Partial> = Vec::new();
    let amp_fund = peaks
        .iter()
        .min_by(|a, b| (a.0 - f_fund).abs().partial_cmp(&(b.0 - f_fund).abs()).unwrap())
        .map(|p| p.1)
        .unwrap_or(1.0)
        .max(1e-9);
    for (freq, amp) in &peaks {
        let ratio = freq / f_fund;
        if ratio < 0.9 {
            continue; // ignore sub-fundamental junk
        }
        let t60 = estimate_t60(sig, fs, *freq);
        partials.push(Partial {
            freq: *freq,
            ratio,
            gain: amp / amp_fund,
            decay: (t60 / t60_fund).clamp(0.02, 4.0),
            t60,
        });
    }
    partials.sort_by(|a, b| a.freq.partial_cmp(&b.freq).unwrap());
    partials
}

/// Track a partial's magnitude across STFT frames and fit a T60 from the decay.
fn estimate_t60(sig: &[f32], fs: u32, freq: f32) -> f32 {
    let nf = 8192usize;
    let hop = 2048usize;
    if sig.len() < nf {
        return 1.0;
    }
    let bin = (freq * nf as f32 / fs as f32).round() as usize;
    if bin < 2 || bin + 2 >= nf / 2 {
        return 1.0;
    }
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(nf);
    let mut env: Vec<f32> = Vec::new();
    let mut start = 0;
    while start + nf <= sig.len() {
        let mut buf = vec![Complex::new(0.0f32, 0.0); nf];
        for i in 0..nf {
            buf[i] = Complex::new(sig[start + i] * hann(i, nf), 0.0);
        }
        fft.process(&mut buf);
        let m = buf[bin - 1..=bin + 1].iter().map(|c| c.norm()).fold(0.0, f32::max);
        env.push(m);
        start += hop;
    }
    if env.len() < 4 {
        return 1.0;
    }
    // Regress log-magnitude vs time from the peak until it drops ~35 dB.
    let peak_idx = env
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    let peak = env[peak_idx].max(1e-12);
    let floor = peak * 10f32.powf(-35.0 / 20.0);
    let dt = hop as f32 / fs as f32;
    let (mut sx, mut sy, mut sxx, mut sxy, mut n) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for i in peak_idx..env.len() {
        if env[i] < floor {
            break;
        }
        let x = ((i - peak_idx) as f32 * dt) as f64;
        let y = (20.0 * (env[i].max(1e-12) / peak).log10()) as f64; // dB, starts ~0
        sx += x;
        sy += y;
        sxx += x * x;
        sxy += x * y;
        n += 1.0;
    }
    if n < 3.0 {
        return 1.0;
    }
    let denom = n * sxx - sx * sx;
    if denom.abs() < 1e-12 {
        return 1.0;
    }
    let slope = (n * sxy - sx * sy) / denom; // dB per second (negative)
    if slope >= -0.1 {
        return 8.0; // essentially not decaying in-window
    }
    ((-60.0 / slope) as f32).clamp(0.1, 20.0)
}

#[inline]
fn hann(i: usize, n: usize) -> f32 {
    let x = core::f32::consts::PI * i as f32 / (n as f32 - 1.0);
    let s = x.sin();
    s * s
}

fn median(v: &mut [f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let m = v.len() / 2;
    if v.len() % 2 == 0 {
        0.5 * (v[m - 1] + v[m])
    } else {
        v[m]
    }
}

impl Clone for Partial {
    fn clone(&self) -> Self {
        Partial { ..*self }
    }
}
impl Copy for Partial {}
