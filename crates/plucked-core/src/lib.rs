//! # plucked-core
//!
//! Plucked-string physical modeling for long-neck lutes — starting with the
//! **çifteli**, the two-string Albanian lute. A çifteli has a **drone** string
//! (rings on a fixed pitch) and a **melody** string (fretted, plays the tune);
//! that maps cleanly onto a modular voice: a fixed drone plus a melody string
//! re-plucked on gate at the incoming (quantized) V/Oct pitch.
//!
//! Each string is an extended **Karplus-Strong** waveguide: a delay line closed
//! by a loss + damping filter, excited by a short noise burst at the pluck. A
//! couple of resonances model the small bright wooden body. Reuses the shared
//! [`puget_dsp`] foundation (fractional delay + math); `no_std`, dependency-free.
//!
//! ```
//! use plucked_core::Cifteli;
//! let mut c = Cifteli::new(48_000.0);
//! c.set_drone_hz(196.0);        // drone string
//! c.pluck_drone(0.8);
//! c.pluck_melody(261.63, 0.9);  // a melody note (already quantized upstream)
//! let (l, r) = c.process();
//! # let _ = (l, r);
//! ```
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![cfg_attr(not(feature = "std"), no_std)]

use puget_dsp::mathf;
use puget_dsp::Delay;

/// A 2-pole resonator (one body mode).
#[derive(Default)]
struct Reso {
    a1: f32,
    a2: f32,
    g: f32,
    y1: f32,
    y2: f32,
}
impl Reso {
    fn new(freq: f32, q: f32, gain: f32, fs: f32) -> Self {
        let w = core::f32::consts::TAU * freq / fs;
        let r = mathf::exp(-core::f32::consts::PI * freq / (q * fs));
        Reso { a1: 2.0 * r * mathf::cos(w), a2: -(r * r), g: gain * mathf::sin(w), y1: 0.0, y2: 0.0 }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.g * x + self.a1 * self.y1 + self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// One extended-Karplus-Strong plucked string.
struct PluckString {
    fs: f32,
    delay: Delay,
    lp_y1: f32,
    /// Loop damping (brightness): higher = darker, faster HF decay.
    damp: f32,
    /// Loop gain (< 1): overall sustain.
    decay: f32,
    /// Noise-burst excitation state.
    rng: u32,
    exc_rem: u32,
    exc_amp: f32,
}
impl PluckString {
    fn new(fs: f32, seed: u32) -> Self {
        let max = (fs / 40.0) as usize + 4; // down to ~40 Hz
        PluckString {
            fs,
            delay: Delay::new(max),
            lp_y1: 0.0,
            damp: 0.5,
            decay: 0.996,
            rng: 0x9E37_79B9 ^ seed,
            exc_rem: 0,
            exc_amp: 0.0,
        }
    }
    #[inline]
    fn white(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
    fn set_freq(&mut self, freq: f32) {
        // Subtract the one-pole loss-filter (~0.5) + cache (1) phase delay.
        self.delay.set_delay((self.fs / freq - 1.5).max(2.0));
    }
    /// Pluck: excite the loop with a noise burst ~one period long.
    fn pluck(&mut self, freq: f32, velocity: f32) {
        self.set_freq(freq);
        self.exc_rem = (self.fs / freq) as u32;
        self.exc_amp = velocity.clamp(0.0, 1.0);
    }
    #[inline]
    fn tick(&mut self) -> f32 {
        let s = self.delay.last_out();
        // Loop loss filter (one-pole low-pass) with sustain gain.
        self.lp_y1 = (1.0 - self.damp) * s + self.damp * self.lp_y1;
        let fed = self.decay * self.lp_y1;
        let exc = if self.exc_rem > 0 {
            self.exc_rem -= 1;
            self.white() * self.exc_amp
        } else {
            0.0
        };
        self.delay.tick(fed + exc);
        s
    }
    #[inline]
    fn active(&self) -> bool {
        self.exc_rem > 0 || self.lp_y1.abs() > 1e-5 || self.delay.last_out().abs() > 1e-5
    }
}

/// The **çifteli**: a drone string + a melody string sharing one bright body.
pub struct Cifteli {
    drone: PluckString,
    melody: PluckString,
    body: [Reso; 2],
    drone_hz: f32,
    body_mix: f32,
}

impl Cifteli {
    pub fn new(fs: f32) -> Self {
        Cifteli {
            drone: PluckString::new(fs, 0x1111),
            melody: PluckString::new(fs, 0x2222),
            // A small, bright boxy wooden body.
            body: [Reso::new(430.0, 12.0, 0.8, fs), Reso::new(1150.0, 9.0, 0.5, fs)],
            drone_hz: 196.0,
            body_mix: 0.12,
        }
    }

    /// Tune the drone string (Hz).
    pub fn set_drone_hz(&mut self, hz: f32) {
        self.drone_hz = hz.max(1.0);
    }

    /// Pluck the drone string (at its tuned pitch).
    pub fn pluck_drone(&mut self, velocity: f32) {
        let hz = self.drone_hz;
        self.drone.pluck(hz, velocity);
    }

    /// Pluck a melody note (Hz) — feed it already-quantized from the upstream
    /// microtonal quantizer.
    pub fn pluck_melody(&mut self, hz: f32, velocity: f32) {
        self.melody.pluck(hz, velocity);
    }

    /// Brightness of both strings: 0 = mellow/dark, 1 = bright and twangy.
    pub fn set_brightness(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        let damp = 0.7 - 0.45 * a;
        self.drone.damp = damp;
        self.melody.damp = damp;
    }

    /// Sustain of both strings: 0 = short/damped, 1 = long ringing.
    pub fn set_sustain(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        let decay = 0.985 + 0.0145 * a;
        self.drone.decay = decay;
        self.melody.decay = decay;
    }

    /// Whether anything is still ringing.
    pub fn active(&self) -> bool {
        self.drone.active() || self.melody.active()
    }

    /// One stereo sample. The two strings are placed slightly apart; the body
    /// colours their sum.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let d = self.drone.tick();
        let m = self.melody.tick();
        let mix = d + m;
        let mut b = 0.0;
        for r in &mut self.body {
            b += r.tick(mix);
        }
        let colored = mix + self.body_mix * b;
        // Drone panned a touch left, melody a touch right, body centred.
        let l = (d * 0.85 + m * 0.5) + 0.5 * self.body_mix * b;
        let r = (d * 0.5 + m * 0.85) + 0.5 * self.body_mix * b;
        let _ = colored;
        (l * 1.3, r * 1.3)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn cifteli_plucks_ring_and_decay() {
        let fs = 48_000.0;
        let mut c = Cifteli::new(fs);
        c.set_brightness(0.6);
        c.set_sustain(0.7);
        c.set_drone_hz(196.0);
        c.pluck_drone(0.8);
        c.pluck_melody(261.63, 0.9);
        let mut peak = 0.0f32;
        for i in 0..48_000 {
            let (l, r) = c.process();
            assert!(l.is_finite() && r.is_finite(), "non-finite");
            if i < 4_000 {
                peak = peak.max(l.abs()).max(r.abs());
            }
        }
        assert!(peak > 0.02, "too quiet: {peak}");
        // Let it ring out; a plucked string decays (no sustain input).
        for _ in 0..48_000 * 6 {
            c.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            let (l, r) = c.process();
            tail = tail.max(l.abs()).max(r.abs());
        }
        assert!(tail < peak, "did not decay: tail {tail} vs peak {peak}");
    }

    #[test]
    fn melody_is_in_tune() {
        let fs = 48_000.0;
        let mut c = Cifteli::new(fs);
        c.set_sustain(1.0);
        c.pluck_melody(220.0, 1.0);
        let buf: Vec<f32> = (0..16_384).map(|_| c.process().0).collect();
        // autocorrelation
        let (lo, hi) = ((fs / 400.0) as usize, (fs / 120.0) as usize);
        let mut best = f32::MIN;
        let mut lag0 = lo;
        for lag in lo..hi {
            let mut s = 0.0;
            for i in 0..buf.len() - lag {
                s += buf[i] * buf[i + lag];
            }
            if s > best {
                best = s;
                lag0 = lag;
            }
        }
        let f = fs / lag0 as f32;
        let cents = 1200.0 * (f / 220.0).log2();
        assert!(cents.abs() < 20.0, "melody off by {cents:.1} cents ({f:.1} Hz)");
    }
}
