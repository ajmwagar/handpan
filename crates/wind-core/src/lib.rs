//! # wind-core
//!
//! Tier-3 physical modeling: **aerophones** — the wind instruments. Like the
//! bowed string these are *self-sustained oscillators*, but the sustaining
//! element is a column of air (a digital-waveguide bore) driven by a nonlinear
//! valve fed by breath pressure.
//!
//! The first voice is the **clarinet**: a single **reed** on a **cylindrical**
//! bore. The bore is a quarter-wave resonator (closed at the reed, open at the
//! bell), so it sounds an octave below an open pipe of the same length and
//! radiates almost purely **odd harmonics** — the hollow, woody clarinet color.
//!
//! Model lineage: Smith / McIntyre–Woodhouse–Schumacher waveguide winds (STK).
//! The bore is a `Delay` (the same primitive the bowed string uses), closed
//! into a loop by a sign-inverting one-zero loss filter, driven by a nonlinear
//! reed table. `no_std`, dependency-free; drops into the same front-ends.
//! Breath pressure, vibrato and brightness are the expression surface
//! (→ CV / MPE / breath controller).
//!
//! ```
//! use wind_core::{Wind, WindKind};
//! let mut c = Wind::new(48_000.0, WindKind::Clarinet);
//! c.note_on(293.66, 0.9);   // D4, breath pressure
//! let s = c.process();
//! c.note_off();
//! # let _ = s;
//! ```
//!
//! Roadmap (later voices reuse this bore + a different exciter): **flute/**
//! **recorder** (air-jet drive, open bore, all harmonics), **saxophone**
//! (reed on a conical bore), **brass** (lip-reed + wave-steepening brassiness).
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

#[allow(dead_code)]
mod mathf;

/// Fractional delay line (linear interpolation) — the bore air column.
struct Delay {
    buf: Vec<f32>,
    w: usize,
    delay: f32,
    last: f32,
}
impl Delay {
    fn new(max_len: usize) -> Self {
        Delay { buf: vec![0.0; max_len.max(4)], w: 0, delay: 1.0, last: 0.0 }
    }
    #[inline]
    fn set_delay(&mut self, d: f32) {
        let max = (self.buf.len() - 2) as f32;
        self.delay = d.clamp(1.0, max);
    }
    #[inline]
    fn last_out(&self) -> f32 {
        self.last
    }
    #[inline]
    fn tick(&mut self, input: f32) -> f32 {
        let len = self.buf.len();
        let mut dr = self.w as f32 - self.delay;
        if dr < 0.0 {
            dr += len as f32;
        }
        let i0 = dr.floor() as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = dr - dr.floor();
        let out = self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac;
        self.buf[self.w] = input;
        self.w = (self.w + 1) % len;
        self.last = out;
        out
    }
}

/// The bore/bell reflection filter: a one-pole low-pass with a loss gain and a
/// sign inversion (the closed reed end reflects with inverted phase). The pole
/// sets how fast the upper (odd) harmonics are rolled off — the difference
/// between a dark, woody clarinet and a buzzy square wave.
struct Reflect {
    /// Loss gain in (0, 1); closer to 1 = longer, more resonant.
    loss: f32,
    /// Low-pass smoothing coefficient in [0, 1); higher = darker.
    damp: f32,
    y1: f32,
}
impl Reflect {
    fn new(loss: f32, damp: f32) -> Self {
        Reflect { loss, damp, y1: 0.0 }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        // One-pole low-pass, then invert and apply loss.
        self.y1 = (1.0 - self.damp) * x + self.damp * self.y1;
        -self.loss * self.y1
    }
}

/// RBJ peaking-EQ biquad — the radiation/formant shaping fit to measured
/// clarinet spectra (the raw waveguide rolls off too fast below the tonehole
/// cutoff; this lifts the strong low-harmonic region back to the real balance).
struct Peaking {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}
impl Peaking {
    fn new(freq: f32, q: f32, db_gain: f32, fs: f32) -> Self {
        let a = mathf::powf(10.0, db_gain / 40.0);
        let w0 = core::f32::consts::TAU * freq / fs;
        let (s, c) = (mathf::sin(w0), mathf::cos(w0));
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha / a;
        Peaking {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * c) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * c) / a0,
            a2: (1.0 - alpha / a) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// The instruments modeled by this core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindKind {
    /// Single reed, cylindrical bore — odd-harmonic, woody.
    Clarinet,
    /// Single reed, conical bore — all harmonics, reedy and bright.
    Saxophone,
    /// Lip reed (buzzing lips) + flared brass bore — all harmonics, with
    /// amplitude-dependent "brassiness" (wave steepening) on loud notes.
    Trumpet,
}

/// A wind instrument: a nonlinear reed exciter driving a bore waveguide.
pub struct Wind {
    fs: f32,
    kind: WindKind,

    bore: Delay,
    refl: Reflect,
    radiate: [Peaking; 2],
    freq: f32,

    // Reed table: opening = clamp(offset + slope·Δp). A one-sided valve
    // (asymmetric clamp) — the reed slams shut but can't invert — so it
    // radiates the weak even harmonics a real clarinet has.
    reed_offset: f32,
    reed_slope: f32,

    // Lip reed (trumpet): a 2-pole mechanical resonator tuned near the note —
    // the player's buzzing lips — driving a one-way (squared) valve. `bright`
    // scales the amplitude-dependent brassiness at the output.
    lip_a1: f32,
    lip_a2: f32,
    lip_b0: f32,
    lip_y1: f32,
    lip_y2: f32,
    bright: f32,

    // Breath pressure, slewed so attacks/releases aren't clicks.
    breath_target: f32,
    breath_env: f32,
    attack_rate: f32,
    release_rate: f32,

    // Breath noise + vibrato.
    rng: u32,
    noise_gain: f32,
    noise_lp: f32,
    vib_phase: f32,
    vib_rate: f32,
    vib_depth: f32,

    // Bell-radiation even-harmonic term (DC-tracked) + output DC blocker.
    even_dc: f32,
    dc_x1: f32,
    dc_y1: f32,

    out_gain: f32,
}

impl Wind {
    pub fn new(fs: f32, kind: WindKind) -> Self {
        let max = (fs / 30.0) as usize + 4; // lowest ~30 Hz bore
        let (reed_offset, reed_slope, out_gain, noise_gain) = match kind {
            // Clarinet reed: classic STK reed table (offset 0.7, slope -0.44).
            WindKind::Clarinet => (0.7, -0.44, 1.0, 0.07),
            // Saxophone reed: same table, a touch grippier; breathier.
            WindKind::Saxophone => (0.7, -0.50, 1.0, 0.10),
            // Trumpet uses the lip resonator, not the reed table.
            WindKind::Trumpet => (0.0, 0.0, 1.0, 0.04),
        };
        // Reflection: clarinet inverts (odd-harmonic, quarter-wave); the
        // conical sax and the brass bore are effectively open (all harmonics),
        // so their loop does not invert. `Reflect::tick` applies `-loss`, so a
        // negative `loss` yields a non-inverting (all-harmonic) loop.
        let (refl_loss, refl_damp) = match kind {
            WindKind::Clarinet => (0.95, 0.81),
            WindKind::Saxophone => (-0.93, 0.70),
            WindKind::Trumpet => (-0.90, 0.55),
        };
        let radiate = match kind {
            // Clarinet: presence lift restoring the sub-cutoff harmonics + a
            // ~3 kHz formant (fit to Iowa MIS spectra).
            WindKind::Clarinet => {
                [Peaking::new(1000.0, 0.6, 13.0, fs), Peaking::new(3000.0, 1.2, 5.0, fs)]
            }
            // Saxophone: a low presence lift (strong fundamental, per the Iowa
            // alto-sax spectrum) and the reedy ~1.6 kHz formant.
            WindKind::Saxophone => {
                [Peaking::new(420.0, 0.6, 6.0, fs), Peaking::new(1500.0, 1.0, 5.0, fs)]
            }
            // Trumpet: the brass formant ("bridge") sits ~1.2–1.5 kHz — that is
            // where the real Iowa trumpet peaks (its h4), not up at 2.4 kHz.
            WindKind::Trumpet => {
                [Peaking::new(1000.0, 0.8, 5.0, fs), Peaking::new(1500.0, 1.1, 6.0, fs)]
            }
        };
        Wind {
            fs,
            kind,
            bore: Delay::new(max),
            refl: Reflect::new(refl_loss, refl_damp),
            radiate,
            freq: 220.0,
            reed_offset,
            reed_slope,
            lip_a1: 0.0,
            lip_a2: 0.0,
            lip_b0: 0.0,
            lip_y1: 0.0,
            lip_y2: 0.0,
            bright: 0.5,
            breath_target: 0.0,
            breath_env: 0.0,
            attack_rate: 0.0,
            release_rate: 0.0,
            rng: 0x2545_F491 ^ (kind as u32).wrapping_mul(0x9E37_79B9),
            noise_gain,
            noise_lp: 0.0,
            vib_phase: 0.0,
            vib_rate: 5.0,
            vib_depth: 0.0,
            even_dc: 0.0,
            dc_x1: 0.0,
            dc_y1: 0.0,
            out_gain,
        }
    }

    fn set_freq(&mut self, freq: f32) {
        self.freq = freq;
        self.update_delay();
    }

    /// Recompute the bore delay from the target frequency and the current loop
    /// filter, so tuning stays correct as `set_brightness` moves the cutoff.
    fn update_delay(&mut self) {
        // The one-pole reflection filter's group delay (≈ damp/(1-damp)) plus
        // the last_out() cache (1 sample), subtracted so tuning stays correct.
        let fgd = self.refl.damp / (1.0 - self.refl.damp);
        let d = match self.kind {
            // Cylindrical quarter-wave (inverting loop): half an open pipe.
            WindKind::Clarinet => self.fs / self.freq * 0.5 - 1.0 - fgd,
            // Conical / brass bores are effectively open (non-inverting loop):
            // a full open pipe → all harmonics.
            WindKind::Saxophone | WindKind::Trumpet => self.fs / self.freq - 1.0 - fgd,
        };
        self.bore.set_delay(d.max(4.0));
        if self.kind == WindKind::Trumpet {
            self.tune_lip(self.freq);
        }
    }

    /// Tune the trumpet's lip resonator to a buzzing frequency near the note.
    fn tune_lip(&mut self, freq: f32) {
        let w = core::f32::consts::TAU * freq / self.fs;
        let q = 6.0;
        let r = mathf::exp(-core::f32::consts::PI * freq / (q * self.fs));
        self.lip_a1 = 2.0 * r * mathf::cos(w);
        self.lip_a2 = -(r * r);
        self.lip_b0 = (1.0 - r) * mathf::sin(w);
    }

    #[inline]
    fn white(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Start a note: `freq` Hz, `breath` pressure in [0, 1].
    pub fn note_on(&mut self, freq: f32, breath: f32) {
        self.set_freq(freq);
        self.set_breath(breath);
        self.attack_rate = 0.0009; // ~12 ms onset
        self.release_rate = 0.002;
    }

    /// Stop blowing — the tone dies quickly (winds have little sustain tail).
    pub fn note_off(&mut self) {
        self.breath_target = 0.0;
    }

    /// Continuous breath control during a note: pressure in [0, 1].
    pub fn set_breath(&mut self, breath: f32) {
        // A pressure range that reliably starts and sustains the exciter
        // without overblowing into the next register.
        let b = breath.clamp(0.0, 1.0);
        self.breath_target = match self.kind {
            WindKind::Clarinet => 0.35 + 0.30 * b,
            WindKind::Saxophone => 0.32 + 0.34 * b,
            WindKind::Trumpet => 0.30 + 0.45 * b,
        };
    }

    /// Vibrato: rate (Hz) and depth (breath-pressure modulation, 0..~0.5).
    pub fn set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        self.vib_rate = rate_hz.max(0.0);
        self.vib_depth = depth.clamp(0.0, 0.5);
    }

    /// Brightness/embouchure: moves the loop low-pass cutoff. 0 = dark and
    /// woody (chalumeau), 1 = bright and reedy (a hard, buzzy embouchure).
    pub fn set_brightness(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        self.bright = a;
        // Moves the loop cutoff (all kinds); for the trumpet it also scales the
        // brassiness applied at the output.
        self.refl.damp = match self.kind {
            WindKind::Clarinet => 0.88 - 0.23 * a,
            WindKind::Saxophone => 0.80 - 0.30 * a,
            WindKind::Trumpet => 0.70 - 0.35 * a,
        };
        self.update_delay(); // cutoff change shifts group delay → keep in tune
    }

    #[inline]
    fn reed(&self, pdiff: f32) -> f32 {
        // Reed opening. A single reed is a one-sided valve: it can be blown
        // fully shut (0) but not driven "inside out". Clamping asymmetrically
        // — hard floor near 0, softer ceiling — breaks the perfect odd-only
        // symmetry of an ideal cylinder and yields the weak even harmonics.
        let r = self.reed_offset + self.reed_slope * pdiff;
        r.clamp(-1.0, 1.0)
    }

    /// One mono sample.
    #[inline]
    pub fn process(&mut self) -> f32 {
        // Slew the breath envelope (attack up, release down).
        let rate = if self.breath_target > self.breath_env {
            self.attack_rate
        } else {
            self.release_rate
        };
        self.breath_env += rate * (self.breath_target - self.breath_env);

        // Breath = envelope + turbulence noise + vibrato (as pressure ripple).
        // The noise is low-passed into an airy "breath" band rather than hiss.
        let n = self.white();
        self.noise_lp += 0.2 * (n - self.noise_lp);
        let mut breath = self.breath_env;
        breath += breath * self.noise_gain * self.noise_lp;
        if self.vib_depth > 0.0 {
            self.vib_phase += core::f32::consts::TAU * self.vib_rate / self.fs;
            if self.vib_phase > core::f32::consts::TAU {
                self.vib_phase -= core::f32::consts::TAU;
            }
            breath += breath * self.vib_depth * 0.1 * mathf::sin(self.vib_phase);
        }

        let mut out = match self.kind {
            // Single reed (clarinet cylindrical / sax conical): the reflected
            // bore pressure drives the nonlinear reed, scattered back in. The
            // clarinet's loop inverts (odd harmonics); the sax's does not (all).
            WindKind::Clarinet | WindKind::Saxophone => {
                let refl = self.refl.tick(self.bore.last_out());
                let pdiff = refl - breath;
                self.bore.tick(breath + pdiff * self.reed(pdiff))
            }
            // Lip reed (trumpet): the buzzing-lip resonator, driven by the
            // pressure across the lips, gates the airflow through a one-way
            // valve into the (all-harmonic) brass bore.
            WindKind::Trumpet => {
                let bore_p = self.refl.tick(self.bore.last_out());
                let delta = breath - bore_p;
                let lip =
                    self.lip_b0 * delta + self.lip_a1 * self.lip_y1 + self.lip_a2 * self.lip_y2;
                self.lip_y2 = self.lip_y1;
                self.lip_y1 = lip;
                // One-way lip valve: the lips open around a rest position and
                // slam shut (can't invert) — the nonlinearity that sustains.
                let area = (lip + 0.7).clamp(0.0, 1.0);
                self.bore.tick(bore_p + area * delta)
            }
        };

        // Brassiness: amplitude-dependent wave steepening — the bright blare a
        // trumpet develops when pushed. A cubic term (odd harmonics) that grows
        // with level; harmless on soft notes, edgy on loud ones.
        if self.kind == WindKind::Trumpet {
            out += 0.10 * self.bright * out * out * out;
        }

        // Bell/tone-hole radiation: a small squared (even-harmonic) term the
        // ideal cylinder can't produce — only the clarinet needs it (the sax
        // and trumpet already carry even harmonics from their open bores).
        if self.kind == WindKind::Clarinet {
            let sq = out * out;
            self.even_dc += 0.0008 * (sq - self.even_dc);
            out += 0.09 * (sq - self.even_dc);
        }
        // Block any residual DC on the way out.
        let y = out - self.dc_x1 + 0.995 * self.dc_y1;
        self.dc_x1 = out;
        self.dc_y1 = y;

        // Radiation/formant shaping (fit to measured spectra).
        let mut r = y;
        for p in &mut self.radiate {
            r = p.tick(r);
        }
        r * self.out_gain
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    /// Autocorrelation pitch estimate.
    fn measured_hz(v: &mut Wind, fs: f32, settle: usize, window: usize) -> f32 {
        for _ in 0..settle {
            v.process();
        }
        let buf: Vec<f32> = (0..window).map(|_| v.process()).collect();
        let lo = (fs / 2000.0) as usize;
        let hi = (fs / 60.0) as usize;
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

    #[test]
    fn clarinet_sounds_and_stops() {
        let fs = 48_000.0;
        let mut v = Wind::new(fs, WindKind::Clarinet);
        v.note_on(293.66, 0.9); // D4
        let mut peak = 0.0f32;
        for i in 0..fs as usize {
            let s = v.process();
            assert!(s.is_finite(), "non-finite");
            if i > fs as usize / 2 {
                peak = peak.max(s.abs());
            }
        }
        assert!(peak > 0.01, "did not sound: {peak}");
        assert!(peak < 20.0, "runaway: {peak}");
        v.note_off();
        for _ in 0..fs as usize {
            v.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            tail = tail.max(v.process().abs());
        }
        assert!(tail < peak, "did not stop after note_off");
    }

    #[test]
    fn clarinet_is_in_tune() {
        let fs = 48_000.0;
        for &(midi_hz, name) in &[(293.66f32, "D4"), (440.0, "A4"), (587.33, "D5")] {
            let mut c = Wind::new(fs, WindKind::Clarinet);
            c.note_on(midi_hz, 0.9);
            let f = measured_hz(&mut c, fs, 24_000, 16_384);
            let cents = 1200.0 * (f / midi_hz).log2();
            assert!(cents.abs() < 25.0, "clarinet {name} off by {cents:.1} cents ({f:.1} Hz)");
        }
    }

    #[test]
    fn clarinet_favors_odd_harmonics() {
        // The hallmark of a cylindrical closed-open bore: the 2nd harmonic is
        // far weaker than the 1st and 3rd. Goertzel magnitudes.
        let fs = 48_000.0;
        let f0 = 293.66f32;
        let mut c = Wind::new(fs, WindKind::Clarinet);
        c.note_on(f0, 0.9);
        for _ in 0..24_000 {
            c.process();
        }
        let n = 16_384;
        let buf: Vec<f32> = (0..n).map(|_| c.process()).collect();
        let mag = |harm: f32| -> f32 {
            let w = core::f32::consts::TAU * (f0 * harm) / fs;
            let (mut s0, mut s1) = (0.0f32, 0.0f32);
            let coeff = 2.0 * w.cos();
            for &x in &buf {
                let s = x + coeff * s0 - s1;
                s1 = s0;
                s0 = s;
            }
            (s0 * s0 + s1 * s1 - coeff * s0 * s1).sqrt()
        };
        let (h1, h2, h3) = (mag(1.0), mag(2.0), mag(3.0));
        assert!(h1 > 0.0 && h3 > 0.0);
        assert!(h2 < 0.5 * (h1 + h3), "not odd-dominant: h1={h1:.3} h2={h2:.3} h3={h3:.3}");
    }

    fn harmonic(buf: &[f32], f0: f32, harm: f32, fs: f32) -> f32 {
        let w = core::f32::consts::TAU * (f0 * harm) / fs;
        let (mut s0, mut s1) = (0.0f32, 0.0f32);
        let coeff = 2.0 * w.cos();
        for &x in buf {
            let s = x + coeff * s0 - s1;
            s1 = s0;
            s0 = s;
        }
        (s0 * s0 + s1 * s1 - coeff * s0 * s1).sqrt()
    }

    #[test]
    fn all_winds_sound_and_stop() {
        let fs = 48_000.0;
        for kind in [WindKind::Clarinet, WindKind::Saxophone, WindKind::Trumpet] {
            let mut v = Wind::new(fs, kind);
            v.note_on(261.63, 0.9); // C4
            let mut peak = 0.0f32;
            for i in 0..fs as usize {
                let s = v.process();
                assert!(s.is_finite(), "{kind:?} non-finite");
                if i > fs as usize / 2 {
                    peak = peak.max(s.abs());
                }
            }
            assert!(peak > 0.01, "{kind:?} did not sound: {peak}");
            assert!(peak < 20.0, "{kind:?} runaway: {peak}");
            v.note_off();
            for _ in 0..fs as usize {
                v.process();
            }
            let mut tail = 0.0f32;
            for _ in 0..4_800 {
                tail = tail.max(v.process().abs());
            }
            assert!(tail < peak, "{kind:?} did not stop");
        }
    }

    #[test]
    fn sax_and_trumpet_in_tune() {
        let fs = 48_000.0;
        for kind in [WindKind::Saxophone, WindKind::Trumpet] {
            for &f0 in &[196.0f32, 293.66, 392.0] {
                let mut v = Wind::new(fs, kind);
                v.note_on(f0, 0.85);
                let f = measured_hz(&mut v, fs, 24_000, 16_384);
                let cents = 1200.0 * (f / f0).log2();
                assert!(cents.abs() < 25.0, "{kind:?} {f0}Hz off by {cents:.1} cents ({f:.1})");
            }
        }
    }

    #[test]
    fn sax_and_trumpet_have_all_harmonics() {
        // Unlike the clarinet, the conical sax and the brass bore radiate the
        // even harmonics strongly — the 2nd harmonic must NOT be suppressed.
        let fs = 48_000.0;
        let f0 = 261.63f32;
        for kind in [WindKind::Saxophone, WindKind::Trumpet] {
            let mut v = Wind::new(fs, kind);
            v.note_on(f0, 0.85);
            for _ in 0..24_000 {
                v.process();
            }
            let buf: Vec<f32> = (0..16_384).map(|_| v.process()).collect();
            let h1 = harmonic(&buf, f0, 1.0, fs);
            let h2 = harmonic(&buf, f0, 2.0, fs);
            let h3 = harmonic(&buf, f0, 3.0, fs);
            let strongest = h1.max(h3).max(1e-9);
            // The even harmonic should be within 20 dB of its odd neighbours
            // (a clarinet would have it 40+ dB down).
            assert!(
                h2 > 0.1 * strongest,
                "{kind:?} missing even harmonics: h1={h1:.3} h2={h2:.3} h3={h3:.3}"
            );
        }
    }
}
