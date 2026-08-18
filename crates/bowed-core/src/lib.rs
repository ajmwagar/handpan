//! # bowed-core
//!
//! Tier-2 physical modeling: the bowed string (violin / viola / cello / bass).
//! Unlike the struck modal instruments, a bowed string is a **self-sustained
//! oscillator** — a digital-waveguide string (two delay lines split at the bow
//! point, with a loss filter) driven by a **nonlinear stick-slip bow-friction**
//! interaction (McIntyre–Woodhouse–Schumacher / Smith–STK). It reuses the same
//! `no_std`, dependency-free discipline and drops into the same front-ends.
//!
//! ```
//! use bowed_core::{Bowed, StringKind};
//! let mut v = Bowed::new(48_000.0, StringKind::Violin);
//! v.note_on(440.0, 0.7, 0.5);        // freq, bow speed, bow pressure
//! let s = v.process();               // one mono sample
//! v.note_off();
//! # let _ = s;
//! ```
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

#[allow(dead_code)]
mod mathf;

/// Fractional delay line (linear interpolation) — one half of the string.
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

/// One-pole low-pass with loss gain — the string's frequency-dependent damping.
struct OnePole {
    pole: f32,
    gain: f32,
    y1: f32,
}
impl OnePole {
    fn new(pole: f32, gain: f32) -> Self {
        OnePole { pole, gain, y1: 0.0 }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        self.y1 = (1.0 - self.pole) * x * self.gain + self.pole * self.y1;
        self.y1
    }
}

/// RBJ band-pass biquad (0 dB peak) — a violin-body formant.
#[derive(Default)]
struct Biquad {
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
impl Biquad {
    fn bandpass(freq: f32, q: f32, fs: f32) -> Self {
        let w0 = core::f32::consts::TAU * freq / fs;
        let (s, c) = (mathf::sin(w0), mathf::cos(w0));
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        Biquad {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alpha) / a0,
            ..Default::default()
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

/// Nonlinear bow friction: differential velocity → friction coefficient in
/// [0, 1]. Higher `slope` (bow pressure) makes the stick region grippier.
#[inline]
fn bow_friction(delta_v: f32, slope: f32) -> f32 {
    let s = (delta_v + 0.001) * slope;
    let mut o = (s.abs() + 0.75).powi(-4);
    if o > 1.0 {
        o = 1.0;
    }
    o
}

/// A bowed-string instrument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringKind {
    Violin,
    Viola,
    Cello,
    Bass,
}

struct BodyDef {
    formants: [(f32, f32); 3], // (freq, Q)
    pole: f32,                 // string loss pole (lower = brighter)
    bow_pos: f32,              // 0..0.5 along the string
}

fn body_def(kind: StringKind) -> BodyDef {
    match kind {
        StringKind::Violin => BodyDef { formants: [(300.0, 2.0), (460.0, 3.0), (700.0, 3.5)], pole: 0.55, bow_pos: 0.13 },
        StringKind::Viola => BodyDef { formants: [(220.0, 2.0), (350.0, 3.0), (600.0, 3.5)], pole: 0.58, bow_pos: 0.12 },
        StringKind::Cello => BodyDef { formants: [(180.0, 2.0), (250.0, 3.0), (450.0, 3.5)], pole: 0.62, bow_pos: 0.11 },
        StringKind::Bass => BodyDef { formants: [(70.0, 2.0), (100.0, 3.0), (180.0, 3.5)], pole: 0.66, bow_pos: 0.10 },
    }
}

pub struct Bowed {
    fs: f32,
    neck: Delay,
    bridge: Delay,
    string_filter: OnePole,
    body: [Biquad; 3],
    bow_pos: f32,
    base_delay: f32,
    // Bow control (slewed so attacks/releases aren't clicks).
    max_vel_target: f32,
    vel_env: f32,
    slope: f32,
    // Vibrato.
    vib_phase: f32,
    vib_rate: f32,
    vib_depth: f32,
    // Texture: bow-friction noise + onset scratch.
    rng: u32,
    noise_lp: f32,
    noise_amt: f32,
    attack: u32,
    // Pizzicato excitation.
    pluck_rem: u32,
    pluck_amp: f32,
}

impl Bowed {
    pub fn new(fs: f32, kind: StringKind) -> Self {
        let d = body_def(kind);
        let max = (fs / 40.0) as usize + 4; // lowest ~40 Hz
        Bowed {
            fs,
            neck: Delay::new(max),
            bridge: Delay::new(max),
            string_filter: OnePole::new(d.pole, 0.95),
            body: [
                Biquad::bandpass(d.formants[0].0, d.formants[0].1, fs),
                Biquad::bandpass(d.formants[1].0, d.formants[1].1, fs),
                Biquad::bandpass(d.formants[2].0, d.formants[2].1, fs),
            ],
            bow_pos: d.bow_pos,
            base_delay: 100.0,
            max_vel_target: 0.0,
            vel_env: 0.0,
            slope: 3.0,
            vib_phase: 0.0,
            vib_rate: 5.5,
            vib_depth: 0.0,
            rng: 0x1234_5678 ^ (kind as u32).wrapping_mul(0x9E37_79B9),
            noise_lp: 0.0,
            noise_amt: 0.12,
            attack: 0,
            pluck_rem: 0,
            pluck_amp: 0.0,
        }
    }

    fn set_freq(&mut self, freq: f32) {
        // Loop length must account for the extra sample of phase delay from the
        // loss filter and the `last_out()` cache, otherwise the pitch runs flat.
        // Empirically ~3.2 samples of excess round-trip delay.
        self.base_delay = (self.fs / freq - 3.2).max(4.0);
        self.retune(0.0);
    }

    #[inline]
    fn white(&mut self) -> f32 {
        // xorshift32 → [-1, 1)
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    #[inline]
    fn retune(&mut self, vib: f32) {
        let d = self.base_delay * (1.0 + vib);
        self.bridge.set_delay(d * self.bow_pos);
        self.neck.set_delay(d * (1.0 - self.bow_pos));
    }

    /// Start a note: `freq` Hz, `bow_velocity` and `bow_pressure` in [0, 1].
    pub fn note_on(&mut self, freq: f32, bow_velocity: f32, bow_pressure: f32) {
        self.set_freq(freq);
        self.set_bow(bow_velocity, bow_pressure);
        // A short burst of extra friction noise as the bow catches the string.
        self.attack = (0.03 * self.fs) as u32; // ~30 ms of onset scratch
    }

    /// Lift the bow — the string rings down.
    pub fn note_off(&mut self) {
        self.max_vel_target = 0.0;
    }

    /// Pizzicato: pluck the string instead of bowing it. `velocity` in [0, 1].
    /// The bow is lifted; the string is excited by a short noise burst injected
    /// into the waveguide, then rings down through the body like a plucked note.
    pub fn pluck(&mut self, freq: f32, velocity: f32) {
        self.set_freq(freq);
        self.max_vel_target = 0.0;
        self.vel_env = 0.0;
        self.pluck_rem = (0.004 * self.fs) as u32; // ~4 ms excitation
        self.pluck_amp = 0.6 * velocity.clamp(0.0, 1.0);
    }

    /// Continuous bow control (during a note): speed and pressure in [0, 1].
    pub fn set_bow(&mut self, bow_velocity: f32, bow_pressure: f32) {
        self.max_vel_target = 0.03 + 0.22 * bow_velocity.clamp(0.0, 1.0);
        self.slope = 1.0 + 4.0 * bow_pressure.clamp(0.0, 1.0);
    }

    /// Bow position along the string: 0 = sul tasto (over the fingerboard,
    /// mellow), 1 = sul ponticello (near the bridge, glassy/bright).
    pub fn set_bow_position(&mut self, pos: f32) {
        // Map [0,1] onto a musical range of the fractional string position.
        self.bow_pos = (0.06 + 0.14 * pos.clamp(0.0, 1.0)).clamp(0.02, 0.5);
        self.retune(0.0);
    }

    /// Timbral brightness: string loss-filter pole. 0 = dark/damped,
    /// 1 = bright/singing. (Also scales bow-friction noise a touch.)
    pub fn set_brightness(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        // Higher brightness → less high-frequency loss (lower pole).
        self.string_filter.pole = 0.72 - 0.30 * a;
        self.noise_amt = 0.06 + 0.14 * a;
    }

    /// Vibrato: rate (Hz) and depth (fraction of a semitone-ish).
    pub fn set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        self.vib_rate = rate_hz.max(0.0);
        self.vib_depth = depth.clamp(0.0, 0.05);
    }

    /// One mono sample.
    #[inline]
    pub fn process(&mut self) -> f32 {
        // Slew the bow velocity (≈8 ms) so on/off isn't a click.
        self.vel_env += 0.0025 * (self.max_vel_target - self.vel_env);

        if self.vib_depth > 0.0 {
            self.vib_phase += core::f32::consts::TAU * self.vib_rate / self.fs;
            if self.vib_phase > core::f32::consts::TAU {
                self.vib_phase -= core::f32::consts::TAU;
            }
            self.retune(self.vib_depth * mathf::sin(self.vib_phase));
        }

        // Pizzicato: inject a short noise burst into the waveguide, no bowing.
        if self.pluck_rem > 0 {
            self.pluck_rem -= 1;
            let exc = self.white() * self.pluck_amp;
            let bridge_refl = -self.string_filter.tick(self.bridge.last_out());
            let nut_refl = -self.neck.last_out();
            self.neck.tick(bridge_refl + exc);
            self.bridge.tick(nut_refl + exc);
            let raw = self.bridge.last_out();
            let mut b = 0.0;
            for f in &mut self.body {
                b += f.tick(raw);
            }
            return (raw * 0.5 + b * 0.9) * 4.0;
        }

        // Bow-friction noise: band-limited turbulence, scaled by how hard the
        // bow is gripping (velocity) plus an onset-scratch burst at the attack.
        let n = self.white();
        self.noise_lp += 0.25 * (n - self.noise_lp); // ~one-pole ≈ 6 kHz
        let mut scratch = 0.0;
        if self.attack > 0 {
            self.attack -= 1;
            scratch = self.noise_lp * 0.5 * (self.attack as f32 / (0.03 * self.fs));
        }
        let noise = self.noise_lp * self.noise_amt * self.vel_env + scratch;

        // String velocity at the bow point from the two returning waves.
        let bridge_refl = -self.string_filter.tick(self.bridge.last_out());
        let nut_refl = -self.neck.last_out();
        let string_vel = bridge_refl + nut_refl;
        let delta = (self.vel_env + noise) - string_vel;
        let new_vel = delta * bow_friction(delta, self.slope);
        self.neck.tick(bridge_refl + new_vel);
        self.bridge.tick(nut_refl + new_vel);

        let raw = self.bridge.last_out();
        // Body coloration: some direct + the formant resonances.
        let mut b = 0.0;
        for f in &mut self.body {
            b += f.tick(raw);
        }
        (raw * 0.5 + b * 0.9) * 4.0
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn bow_sustains_then_stops_and_stays_bounded() {
        for kind in [StringKind::Violin, StringKind::Cello, StringKind::Bass] {
            let mut v = Bowed::new(48_000.0, kind);
            let freq = match kind {
                StringKind::Bass => 55.0,
                StringKind::Cello => 130.0,
                _ => 440.0,
            };
            v.note_on(freq, 0.7, 0.5);
            // Bow for 1 s — should build a sustained tone.
            let mut peak_bowed = 0.0f32;
            for i in 0..48_000 {
                let s = v.process();
                assert!(s.is_finite(), "{kind:?} non-finite");
                if i > 24_000 {
                    peak_bowed = peak_bowed.max(s.abs());
                }
            }
            assert!(peak_bowed > 0.02, "{kind:?} did not sustain: {peak_bowed}");
            assert!(peak_bowed < 50.0, "{kind:?} runaway: {peak_bowed}");
            // Lift the bow — it should decay.
            v.note_off();
            for _ in 0..48_000 * 2 {
                v.process();
            }
            let mut tail = 0.0f32;
            for _ in 0..4_800 {
                tail = tail.max(v.process().abs());
            }
            assert!(tail < peak_bowed, "{kind:?} did not decay after note_off");
        }
    }

    /// Measure the fundamental via autocorrelation and confirm we're in tune.
    fn measured_hz(v: &mut Bowed, fs: f32, settle: usize, window: usize) -> f32 {
        for _ in 0..settle {
            v.process();
        }
        let buf: Vec<f32> = (0..window).map(|_| v.process()).collect();
        // Autocorrelation peak search over a musical lag range (60–1200 Hz).
        let lo = (fs / 1200.0) as usize;
        let hi = (fs / 60.0) as usize;
        let mut best_lag = lo;
        let mut best = f32::MIN;
        for lag in lo..hi.min(window / 2) {
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
    fn tuning_is_accurate() {
        let fs = 48_000.0;
        // A4 on the violin.
        let mut v = Bowed::new(fs, StringKind::Violin);
        v.note_on(440.0, 0.7, 0.5);
        let f = measured_hz(&mut v, fs, 24_000, 8_192);
        let cents = 1200.0 * (f / 440.0).log2();
        assert!(cents.abs() < 15.0, "violin A4 off by {cents:.1} cents ({f:.1} Hz)");

        // C3 on the cello.
        let mut c = Bowed::new(fs, StringKind::Cello);
        c.note_on(130.81, 0.7, 0.5);
        let f = measured_hz(&mut c, fs, 24_000, 16_384);
        let cents = 1200.0 * (f / 130.81).log2();
        assert!(cents.abs() < 15.0, "cello C3 off by {cents:.1} cents ({f:.1} Hz)");
    }

    #[test]
    fn pizzicato_rings_and_decays() {
        let fs = 48_000.0;
        let mut v = Bowed::new(fs, StringKind::Cello);
        v.pluck(196.0, 0.9);
        let mut peak = 0.0f32;
        for _ in 0..2_400 {
            peak = peak.max(v.process().abs());
        }
        assert!(peak > 0.02, "pluck too quiet: {peak}");
        // Let it ring out; it must not self-sustain (no bow energy).
        for _ in 0..fs as usize * 2 {
            v.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            tail = tail.max(v.process().abs());
        }
        assert!(tail < peak * 0.5, "pluck did not decay: tail {tail} vs peak {peak}");
    }
}
