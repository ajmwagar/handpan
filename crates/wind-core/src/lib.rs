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

/// One-zero averaging low-pass — the bore/bell reflection loss filter.
/// `y = 0.5·gain·(x + x[n-1])`. A negative `gain` inverts (closed-end reflect).
struct OneZero {
    gain: f32,
    x1: f32,
}
impl OneZero {
    fn new(gain: f32) -> Self {
        OneZero { gain, x1: 0.0 }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = 0.5 * self.gain * (x + self.x1);
        self.x1 = x;
        y
    }
}

/// The instruments modeled by this core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindKind {
    /// Single reed, cylindrical bore — odd-harmonic, woody.
    Clarinet,
}

/// A wind instrument: a nonlinear reed exciter driving a bore waveguide.
pub struct Wind {
    fs: f32,
    kind: WindKind,

    bore: Delay,
    refl: OneZero,

    // Reed table: out = clamp(offset + slope·Δp, -1, 1).
    reed_offset: f32,
    reed_slope: f32,

    // Breath pressure, slewed so attacks/releases aren't clicks.
    breath_target: f32,
    breath_env: f32,
    attack_rate: f32,
    release_rate: f32,

    // Breath noise + vibrato.
    rng: u32,
    noise_gain: f32,
    vib_phase: f32,
    vib_rate: f32,
    vib_depth: f32,

    out_gain: f32,
}

impl Wind {
    pub fn new(fs: f32, kind: WindKind) -> Self {
        let max = (fs / 40.0) as usize + 4; // lowest ~40 Hz bore
        let (reed_offset, reed_slope, out_gain, noise_gain) = match kind {
            // Clarinet reed: grippy negative-slope table (STK-style).
            WindKind::Clarinet => (0.7, -0.44, 1.0, 0.02),
        };
        Wind {
            fs,
            kind,
            bore: Delay::new(max),
            refl: OneZero::new(-0.95),
            reed_offset,
            reed_slope,
            breath_target: 0.0,
            breath_env: 0.0,
            attack_rate: 0.0,
            release_rate: 0.0,
            rng: 0x2545_F491 ^ (kind as u32).wrapping_mul(0x9E37_79B9),
            noise_gain,
            vib_phase: 0.0,
            vib_rate: 5.0,
            vib_depth: 0.0,
            out_gain,
        }
    }

    fn set_freq(&mut self, freq: f32) {
        match self.kind {
            WindKind::Clarinet => {
                // Cylindrical quarter-wave: half the samples of an open pipe.
                // Subtract the one-zero (0.5) + delay-cache (1) phase delay.
                let d = (self.fs / freq * 0.5 - 1.5).max(4.0);
                self.bore.set_delay(d);
            }
        }
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
        // A pressure range that reliably starts and sustains the reed without
        // overblowing into the next register.
        self.breath_target = 0.35 + 0.30 * breath.clamp(0.0, 1.0);
    }

    /// Vibrato: rate (Hz) and depth (breath-pressure modulation, 0..~0.5).
    pub fn set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        self.vib_rate = rate_hz.max(0.0);
        self.vib_depth = depth.clamp(0.0, 0.5);
    }

    /// Brightness/embouchure: reflection-filter gain. 0 = dark, 1 = bright.
    pub fn set_brightness(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        self.refl.gain = -0.98 + 0.10 * a;
    }

    #[inline]
    fn reed(&self, pdiff: f32) -> f32 {
        let mut r = self.reed_offset + self.reed_slope * pdiff;
        if r > 1.0 {
            r = 1.0;
        } else if r < -1.0 {
            r = -1.0;
        }
        r
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
        let mut breath = self.breath_env;
        breath += breath * self.noise_gain * self.white();
        if self.vib_depth > 0.0 {
            self.vib_phase += core::f32::consts::TAU * self.vib_rate / self.fs;
            if self.vib_phase > core::f32::consts::TAU {
                self.vib_phase -= core::f32::consts::TAU;
            }
            breath += breath * self.vib_depth * 0.1 * mathf::sin(self.vib_phase);
        }

        // Reflected bore pressure through the loss filter (sign-inverting),
        // then the nonlinear reed scatters the pressure difference back in.
        let refl = self.refl.tick(self.bore.last_out());
        let pdiff = refl - breath;
        let out = self.bore.tick(breath + pdiff * self.reed(pdiff));
        out * self.out_gain
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
}
