//! A/B measurement battery for the STATIC vs DYNAMIC reed spike.
//!
//! Renders sustained notes of the clarinet and sorna and reports, per note:
//!   - h1..h8 harmonic amplitudes (Goertzel single-bin DFT), dB rel strongest
//!   - autocorrelation pitch error in cents vs the target f0
//!   - RMS level and a stability metric (2nd-half RMS / 1st-half RMS)
//! plus a register x breath sweep flagging silent/choked or pitch-unstable notes.
//!
//! `cargo run -q -p wind-core --example reed_ab -- <mode>`
//!   mode = static | dynamic | both   (default both)
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.
use wind_core::{Wind, WindKind};

const FS: f32 = 44_100.0;

fn midi_hz(m: f32) -> f32 {
    440.0 * 2f32.powf((m - 69.0) / 12.0)
}

/// Render a sustained note and return the samples after the onset settles.
fn render(kind: WindKind, f0: f32, bright: f32, breath: f32, dynamic: bool, secs: f32) -> Vec<f32> {
    let mut v = Wind::new(FS, kind);
    v.set_dynamic_reed(dynamic);
    v.set_brightness(bright);
    v.note_on(f0, breath);
    let n = (secs * FS) as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(v.process());
    }
    out
}

/// Goertzel single-bin magnitude at frequency `f`.
fn goertzel(x: &[f32], f: f32, fs: f32) -> f32 {
    let w = core::f32::consts::TAU * f / fs;
    let cw = w.cos();
    let coeff = 2.0 * cw;
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s0 = v + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let real = s1 - s2 * cw;
    let imag = s2 * w.sin();
    (real * real + imag * imag).sqrt() / (x.len() as f32 * 0.5)
}

/// h1..h8 in dB relative to the strongest harmonic.
fn harmonics_db(x: &[f32], f0: f32) -> [f32; 8] {
    let mut mag = [0.0f32; 8];
    for k in 0..8 {
        mag[k] = goertzel(x, f0 * (k as f32 + 1.0), FS);
    }
    let peak = mag.iter().cloned().fold(1e-12f32, f32::max);
    let mut db = [0.0f32; 8];
    for k in 0..8 {
        db[k] = 20.0 * (mag[k] / peak).max(1e-6).log10();
    }
    db
}

/// Autocorrelation pitch (Hz) searched near the expected f0, parabolic-refined.
fn acf_pitch(x: &[f32], f0: f32) -> f32 {
    let p0 = FS / f0;
    let lo = ((p0 * 0.5) as usize).max(2);
    let hi = ((p0 * 2.0) as usize).min(x.len() / 2);
    if hi <= lo + 2 {
        return 0.0;
    }
    let mut best_lag = lo;
    let mut best = f32::MIN;
    for lag in lo..hi {
        let mut acc = 0.0f32;
        for i in 0..(x.len() - lag) {
            acc += x[i] * x[i + lag];
        }
        if acc > best {
            best = acc;
            best_lag = lag;
        }
    }
    // Parabolic interpolation around the peak lag.
    let corr = |lag: usize| -> f32 {
        let mut acc = 0.0f32;
        for i in 0..(x.len() - lag) {
            acc += x[i] * x[i + lag];
        }
        acc
    };
    let lag = best_lag as f32;
    if best_lag > lo && best_lag < hi - 1 {
        let ym1 = corr(best_lag - 1);
        let y0 = best;
        let yp1 = corr(best_lag + 1);
        let denom = ym1 - 2.0 * y0 + yp1;
        let delta = if denom.abs() > 1e-9 { 0.5 * (ym1 - yp1) / denom } else { 0.0 };
        return FS / (lag + delta);
    }
    FS / lag
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|&v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

fn cents(f_meas: f32, f0: f32) -> f32 {
    if f_meas <= 0.0 {
        return f32::NAN;
    }
    1200.0 * (f_meas / f0).log2()
}

fn label(kind: WindKind) -> &'static str {
    match kind {
        WindKind::Clarinet => "clarinet",
        WindKind::Sorna => "sorna",
        _ => "other",
    }
}

/// Detailed single-note report at a nominal playing point.
fn report_note(kind: WindKind, midi: f32, bright: f32, breath: f32, dynamic: bool) {
    let f0 = midi_hz(midi);
    let full = render(kind, f0, bright, breath, dynamic, 1.5);
    // Analysis window: skip the first 0.5 s (onset/bloom), take 0.75 s.
    let start = (0.5 * FS) as usize;
    let win = &full[start..(start + (0.75 * FS) as usize).min(full.len() - start)];
    let db = harmonics_db(win, f0);
    let fpit = acf_pitch(win, f0);
    let ce = cents(fpit, f0);
    // Stability: 2nd-half vs 1st-half RMS of the whole tail (blow-up / decay / choke).
    let h = win.len() / 2;
    let r1 = rms(&win[..h]);
    let r2 = rms(&win[h..]);
    let stab = if r1 > 1e-6 { r2 / r1 } else { 0.0 };
    let peak = win.iter().cloned().fold(0.0f32, |m, v| m.max(v.abs()));
    print!(
        "{:8} m{:<3} b{:.2} br{:.2} | RMS {:.4} pk {:.3} stab {:.2} | pitch {:+6.1}c | h1..8 ",
        label(kind), midi as i32, breath, bright, rms(win), peak, stab, ce
    );
    for k in 0..8 {
        print!("{:5.0} ", db[k]);
    }
    println!();
}

/// Register x breath stability sweep: flags choked (silent) and pitch-off notes.
fn sweep(kind: WindKind, midis: &[f32], breaths: &[f32], bright: f32, dynamic: bool) {
    println!(
        "  sweep {} [{}]  (cell = RMS/100 . pitch-cents, X=choked <2e-3 RMS, !=|cents|>35)",
        label(kind),
        if dynamic { "DYNAMIC" } else { "static" }
    );
    print!("    midi\\breath");
    for b in breaths {
        print!("   {:.2}   ", b);
    }
    println!();
    for &m in midis {
        let f0 = midi_hz(m);
        print!("    m{:<3}      ", m as i32);
        for &b in breaths {
            let full = render(kind, f0, bright, b, dynamic, 1.0);
            let start = (0.4 * FS) as usize;
            let win = &full[start..];
            let r = rms(win);
            let fpit = acf_pitch(win, f0);
            let ce = cents(fpit, f0);
            let flag = if r < 2e-3 {
                'X'
            } else if ce.abs() > 35.0 {
                '!'
            } else {
                ' '
            };
            print!("{:4.0}.{:+04.0}{} ", r * 100.0, ce, flag);
        }
        println!();
    }
}

fn battery(dynamic: bool) {
    let tag = if dynamic { "DYNAMIC REED" } else { "STATIC REED (baseline)" };
    println!("\n================ {} ================", tag);
    // Detailed notes across each register, mid breath + brightness sweep.
    for &kind in &[WindKind::Clarinet, WindKind::Sorna] {
        println!("\n-- {} detailed notes --", label(kind));
        let midis: &[f32] = if kind == WindKind::Clarinet {
            &[50.0, 57.0, 62.0, 69.0, 74.0]
        } else {
            &[62.0, 67.0, 72.0, 76.0, 79.0]
        };
        for &m in midis {
            for &br in &[0.2f32, 0.5, 0.9] {
                report_note(kind, m, br, 0.7, dynamic);
            }
        }
    }
    // Stability sweeps.
    println!();
    let cl_m = [46.0, 50.0, 55.0, 60.0, 65.0, 70.0, 74.0];
    let so_m = [60.0, 64.0, 67.0, 72.0, 76.0, 79.0, 83.0];
    let breaths = [0.1f32, 0.3, 0.5, 0.7, 0.9, 1.0];
    sweep(WindKind::Clarinet, &cl_m, &breaths, 0.5, dynamic);
    println!();
    sweep(WindKind::Sorna, &so_m, &breaths, 0.5, dynamic);
}

/// Grid-search the dynamic-reed (comp, wflow) space for a voice at one note,
/// reporting RMS and pitch-cents so a self-oscillating fundamental is visible.
fn grid(kind: WindKind, midi: f32, hz: f32, zeta: f32) {
    let f0 = midi_hz(midi);
    println!(
        "grid {} m{} reed_hz {} zeta {}  (cell = RMS/100 . pitch-cents)",
        label(kind), midi as i32, hz, zeta
    );
    let comps = [0.8f32, 1.0, 1.2, 1.5, 2.0];
    let wflows = [1.05f32, 1.1, 1.15, 1.2, 1.3, 1.4];
    print!("  comp\\wflow ");
    for w in wflows {
        print!("   {:.1}    ", w);
    }
    println!();
    for &c in &comps {
        print!("  {:.1}      ", c);
        for &w in &wflows {
            let mut v = Wind::new(FS, kind);
            v.set_dynamic_reed(true);
            v.set_reed_params(hz, zeta, c, w);
            v.set_brightness(0.5);
            v.note_on(f0, 0.7);
            let n = (1.0 * FS) as usize;
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                out.push(v.process());
            }
            let win = &out[(0.4 * FS) as usize..];
            let r = rms(win);
            let ce = cents(acf_pitch(win, f0), f0);
            print!("{:4.0}.{:+05.0} ", r * 100.0, ce);
        }
        println!();
    }
}

/// Breath-response probe: for a grid of (comp, wflow), report RMS at low / mid /
/// high breath so the (desired) monotone-increasing loudness-vs-breath shows up.
fn breath_probe(kind: WindKind, midi: f32, hz: f32, zeta: f32) {
    let f0 = midi_hz(midi);
    println!(
        "breath {} m{} reed_hz {} zeta {}  (cell = RMS@b0.2 / b0.6 / b1.0, ×100)",
        label(kind), midi as i32, hz, zeta
    );
    let comps = [0.4f32, 0.6, 0.8, 1.0];
    let wflows = [1.1f32, 1.2, 1.35, 1.5, 1.7];
    print!("  comp\\wflow ");
    for w in wflows {
        print!("     {:.2}       ", w);
    }
    println!();
    for &c in &comps {
        print!("  {:.1}      ", c);
        for &w in &wflows {
            let mut r = [0.0f32; 3];
            for (i, &b) in [0.2f32, 0.6, 1.0].iter().enumerate() {
                let mut v = Wind::new(FS, kind);
                v.set_dynamic_reed(true);
                v.set_reed_params(hz, zeta, c, w);
                v.set_brightness(0.5);
                v.note_on(f0, b);
                let n = (0.9 * FS) as usize;
                let mut out = Vec::with_capacity(n);
                for _ in 0..n {
                    out.push(v.process());
                }
                r[i] = rms(&out[(0.4 * FS) as usize..]);
            }
            print!("{:3.0}/{:3.0}/{:3.0}  ", r[0] * 100.0, r[1] * 100.0, r[2] * 100.0);
        }
        println!();
    }
}

/// Write a normalized 16-bit mono WAV.
fn write_wav(path: &str, samples: &[f32]) {
    use std::io::Write;
    let peak = samples.iter().fold(1e-9f32, |m, &x| m.max(x.abs()));
    let g = 0.9 / peak;
    let n = samples.len();
    let (ch, bits, sr) = (1u16, 16u16, FS as u32);
    let ba = ch * bits / 8;
    let dl = (n as u32) * ba as u32;
    let mut w = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    w.write_all(b"RIFF").unwrap();
    w.write_all(&(36 + dl).to_le_bytes()).unwrap();
    w.write_all(b"WAVE").unwrap();
    w.write_all(b"fmt ").unwrap();
    w.write_all(&16u32.to_le_bytes()).unwrap();
    w.write_all(&1u16.to_le_bytes()).unwrap();
    w.write_all(&ch.to_le_bytes()).unwrap();
    w.write_all(&sr.to_le_bytes()).unwrap();
    w.write_all(&(sr * ba as u32).to_le_bytes()).unwrap();
    w.write_all(&ba.to_le_bytes()).unwrap();
    w.write_all(&bits.to_le_bytes()).unwrap();
    w.write_all(b"data").unwrap();
    w.write_all(&dl.to_le_bytes()).unwrap();
    for &s in samples {
        w.write_all(&(((s * g).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).unwrap();
    }
}

/// Render a short expressive phrase (crescendo across a few notes) to WAV.
fn render_wav(kind: WindKind, dynamic: bool, path: &str) {
    let notes: &[f32] = if kind == WindKind::Clarinet {
        &[50.0, 55.0, 57.0, 62.0, 60.0, 55.0]
    } else {
        &[62.0, 67.0, 69.0, 74.0, 72.0, 67.0]
    };
    let mut v = Wind::new(FS, kind);
    v.set_dynamic_reed(dynamic);
    v.set_brightness(0.6);
    let mut out = Vec::new();
    let note_len = (0.55 * FS) as usize;
    for (i, &m) in notes.iter().enumerate() {
        v.note_on(midi_hz(m), 0.4);
        for k in 0..note_len {
            // Gentle crescendo/decrescendo within each note.
            let b = 0.3 + 0.6 * (k as f32 / note_len as f32).min(1.0);
            if k % 256 == 0 {
                v.set_breath(b);
            }
            out.push(v.process());
        }
        if i == notes.len() - 1 {
            v.note_off();
            for _ in 0..(0.3 * FS) as usize {
                out.push(v.process());
            }
        }
    }
    write_wav(path, &out);
    println!("wrote {path}");
}

/// Sorna hardening probe: brightness→spectrum, register×breath peak (clip check),
/// and an attack-transient check. Uses the DEFAULT construction (dynamic reed on).
fn sorna_harden() {
    let kind = WindKind::Sorna;
    // (a) brightness sweep: h1..h8 + h2-vs-strongest ratio (the all-harmonic test).
    println!("brightness sweep  m60 (C4), breath 0.9   (h1..8 dB rel strongest; h2/strong ratio)");
    for i in 0..=10 {
        let a = i as f32 / 10.0;
        let f0 = midi_hz(60.0);
        let mut v = Wind::new(FS, kind);
        v.set_brightness(a);
        v.note_on(f0, 0.9);
        let n = (1.2 * FS) as usize;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(v.process());
        }
        let win = &out[(0.5 * FS) as usize..];
        let db = harmonics_db(win, f0);
        // Absolute h1/h2/h3 magnitudes for the ratio the unit test uses.
        let h1 = goertzel(win, f0, FS);
        let h2 = goertzel(win, f0 * 2.0, FS);
        let h3 = goertzel(win, f0 * 3.0, FS);
        let ratio = h2 / h1.max(h3).max(1e-9);
        print!("  br{:.1} | ", a);
        for k in 0..8 {
            print!("{:4.0} ", db[k]);
        }
        println!("| h2/strong {:.3} {}", ratio, if ratio > 0.1 { "PASS" } else { "FAIL" });
    }
    // (b) register x breath peak (pre-normalization clip check).
    println!("\nregister x breath PEAK (want < ~1.0)   brightness 0.6");
    let midis = [55.0f32, 60.0, 64.0, 67.0, 72.0, 76.0, 79.0, 83.0];
    let breaths = [0.35f32, 0.55, 0.75, 0.9, 1.0];
    print!("  midi\\breath");
    for b in breaths {
        print!("  {:.2} ", b);
    }
    println!();
    for &m in &midis {
        let f0 = midi_hz(m);
        print!("  m{:<3}     ", m as i32);
        for &b in &breaths {
            let mut v = Wind::new(FS, kind);
            v.set_brightness(0.6);
            v.note_on(f0, b);
            let n = (1.0 * FS) as usize;
            let mut pk = 0.0f32;
            for _ in 0..n {
                pk = pk.max(v.process().abs());
            }
            print!(" {:.2} ", pk);
        }
        println!();
    }
    // (c) attack transient: onset peak (first 30 ms) vs steady peak (0.5..1.0 s).
    println!("\nattack transient (onset-pk / steady-pk should be ~1, not a spike)  brightness 0.6");
    for &m in &[55.0f32, 62.0, 67.0, 74.0, 79.0] {
        let f0 = midi_hz(m);
        let mut v = Wind::new(FS, kind);
        v.set_brightness(0.6);
        v.note_on(f0, 0.9);
        let mut onset_pk = 0.0f32;
        let mut steady_pk = 0.0f32;
        for i in 0..(1.0 * FS) as usize {
            let s = v.process().abs();
            if (i as f32) < 0.03 * FS {
                onset_pk = onset_pk.max(s);
            }
            if (i as f32) > 0.5 * FS {
                steady_pk = steady_pk.max(s);
            }
        }
        println!(
            "  m{:<3} onset-pk {:.3}  steady-pk {:.3}  ratio {:.2}",
            m as i32, onset_pk, steady_pk, onset_pk / steady_pk.max(1e-9)
        );
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "both".into());
    match mode.as_str() {
        "static" => battery(false),
        "dynamic" => battery(true),
        "grid" => {
            grid(WindKind::Clarinet, 50.0, 2600.0, 0.40);
            println!();
            grid(WindKind::Clarinet, 62.0, 2600.0, 0.40);
            println!();
            grid(WindKind::Sorna, 67.0, 3200.0, 0.32);
            println!();
            grid(WindKind::Sorna, 62.0, 3200.0, 0.32);
        }
        "breath" => {
            breath_probe(WindKind::Clarinet, 50.0, 2600.0, 0.40);
            println!();
            breath_probe(WindKind::Sorna, 67.0, 3200.0, 0.32);
        }
        "sorna" => sorna_harden(),
        "wav" => {
            let dir = std::env::args().nth(2).unwrap_or_else(|| ".".into());
            render_wav(WindKind::Clarinet, false, &format!("{dir}/clarinet_static.wav"));
            render_wav(WindKind::Clarinet, true, &format!("{dir}/clarinet_dynamic.wav"));
            render_wav(WindKind::Sorna, false, &format!("{dir}/sorna_static.wav"));
            render_wav(WindKind::Sorna, true, &format!("{dir}/sorna_dynamic.wav"));
        }
        _ => {
            battery(false);
            battery(true);
        }
    }
}
