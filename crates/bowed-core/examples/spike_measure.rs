//! SPIKE measurement harness for the bowed-string exciter reshaping.
//!
//! Renders sustained notes across a pressure x bow-velocity x StringKind grid,
//! measures harmonics h1..h8 (Goertzel, dB relative to h1), pitch (cents error,
//! via autocorrelation), loudness (rms), peak, and finiteness. Prints a
//! plain-text A/B battery. Render-only; writes no audio.
//!
//! `cargo run -p bowed-core --example spike_measure`
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use bowed_core::{Bowed, StringKind};

const FS: f32 = 48_000.0;

/// Goertzel magnitude at frequency `f` over `buf`.
fn goertzel(buf: &[f32], f: f32, fs: f32) -> f32 {
    let w = std::f32::consts::TAU * f / fs;
    let cw = w.cos();
    let coeff = 2.0 * cw;
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &x in buf {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let real = s1 - s2 * cw;
    let imag = s2 * w.sin();
    (real * real + imag * imag).sqrt() / (buf.len() as f32 * 0.5)
}

/// Autocorrelation pitch estimate (Hz), searched within +/- ~40% of the
/// expected fundamental so a complex waveform's double-period doesn't read as a
/// spurious octave-down detune.
fn measured_hz(buf: &[f32], fs: f32, expected: f32) -> f32 {
    let lo = (fs / (expected * 1.45)) as usize;
    let hi = (fs / (expected * 0.7)) as usize;
    let window = buf.len();
    let mut best_lag = lo.max(1);
    let mut best = f32::MIN;
    for lag in lo.max(1)..hi.min(window / 2) {
        let mut s = 0.0;
        for i in 0..window - lag {
            s += buf[i] * buf[i + lag];
        }
        if s > best {
            best = s;
            best_lag = lag;
        }
    }
    fs / best_lag as f32
}

struct Meas {
    rms: f32,
    peak: f32,
    finite: bool,
    cents: f32,
    h_db: [f32; 8], // relative to h1
}

fn measure(kind: StringKind, freq: f32, bv: f32, pressure: f32) -> Meas {
    let mut v = Bowed::new(FS, kind);
    v.note_on(freq, bv, pressure);
    let mut finite = true;
    // settle
    for _ in 0..24_000 {
        if !v.process().is_finite() {
            finite = false;
        }
    }
    let window = 16_384;
    let mut buf = Vec::with_capacity(window);
    let (mut acc, mut peak) = (0.0f64, 0.0f32);
    for _ in 0..window {
        let s = v.process();
        if !s.is_finite() {
            finite = false;
        }
        peak = peak.max(s.abs());
        acc += (s as f64) * (s as f64);
        buf.push(if s.is_finite() { s } else { 0.0 });
    }
    let rms = (acc / window as f64).sqrt() as f32;
    let hz = measured_hz(&buf, FS, freq);
    let cents = 1200.0 * (hz / freq).log2();
    let mut mags = [0.0f32; 8];
    for (i, m) in mags.iter_mut().enumerate() {
        *m = goertzel(&buf, freq * (i as f32 + 1.0), FS);
    }
    let h1 = mags[0].max(1e-12);
    let mut h_db = [0.0f32; 8];
    for i in 0..8 {
        h_db[i] = 20.0 * (mags[i].max(1e-12) / h1).log10();
    }
    Meas { rms, peak, finite, cents, h_db }
}

fn main() {
    let cases: &[(StringKind, f32, &str)] = &[
        (StringKind::Violin, 440.0, "Violin A4"),
        (StringKind::Cello, 220.0, "Cello A3"),
    ];
    let bvs = [0.7f32];
    let pressures = [0.25f32, 0.5, 0.75, 1.0];

    println!("=== HARMONIC BATTERY (h1..h8, dB re h1) ===");
    for &(kind, freq, name) in cases {
        for &bv in &bvs {
            for &p in &pressures {
                let m = measure(kind, freq, bv, p);
                print!("{name} bv{bv} p{p:.2}: ");
                for i in 0..8 {
                    print!("h{}={:+6.1} ", i + 1, m.h_db[i]);
                }
                println!(
                    "| rms={:.2} peak={:.2} cents={:+.1} fin={}",
                    m.rms, m.peak, m.cents, m.finite
                );
            }
        }
    }

    println!("\n=== STABILITY GRID (all kinds, pressure x bowvel) ===");
    let grid_kinds: &[(StringKind, f32)] = &[
        (StringKind::Violin, 440.0),
        (StringKind::Viola, 293.66),
        (StringKind::Cello, 130.81),
        (StringKind::Bass, 55.0),
        (StringKind::Kamancheh, 392.0),
        (StringKind::Fiddle, 440.0),
    ];
    let gpress = [0.0f32, 0.25, 0.5, 0.75, 1.0];
    let gbv = [0.4f32, 0.7, 1.0];
    let mut worst_cents = 0.0f32;
    let mut any_bad = false;
    for &(kind, freq) in grid_kinds {
        for &bv in &gbv {
            let mut rmss = [0.0f32; 5];
            let mut line = format!("{kind:?} bv{bv}: ");
            let mut runbad = false;
            for (i, &p) in gpress.iter().enumerate() {
                let m = measure(kind, freq, bv, p);
                rmss[i] = m.rms;
                worst_cents = worst_cents.max(m.cents.abs());
                let bad = !m.finite || m.peak >= 80.0 || m.rms <= 0.5;
                if bad {
                    runbad = true;
                    any_bad = true;
                }
                line.push_str(&format!(
                    "p{p:.2}(rms{:.1},pk{:.1},c{:+.0}){} ",
                    m.rms,
                    m.peak,
                    m.cents,
                    if bad { "!!" } else { "" }
                ));
            }
            // monotonicity vs running max (TOL 0.80)
            let mut run = -1.0f32;
            let mut mono = true;
            for &r in &rmss {
                if run >= 0.0 && r < run * 0.80 {
                    mono = false;
                }
                run = run.max(r);
            }
            line.push_str(if mono { "[mono OK]" } else { "[MONO FAIL]" });
            if !mono {
                any_bad = true;
            }
            let _ = runbad;
            println!("{line}");
        }
    }
    println!("\nworst |cents| across grid: {worst_cents:.1}");
    println!("grid verdict: {}", if any_bad { "PROBLEMS" } else { "ALL CLEAN" });
}
