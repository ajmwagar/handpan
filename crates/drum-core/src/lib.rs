//! # drum-core
//!
//! Membrane-drum physical modeling — a **bass drum** of the Persian *dohol* /
//! Turkish–Balkan *davul* family: a big double-headed drum slung on the body,
//! struck with a heavy beater on one side (the deep boom) and a thin switch on
//! the other (the sharp crack). Culturally it is the outdoor bass voice paired
//! with the *sorna*; retuned, the same model covers the davul, the taiko, or a
//! kit bass drum.
//!
//! A real drumhead is a **2D circular membrane**, so its partials are the Bessel
//! modes — *inharmonic* ratios (1 : 1.59 : 2.14 : 2.30 : …), nothing like a
//! string's integer harmonics. We synthesize them additively as decaying
//! sinusoids, which makes two things that define a bass drum cheap and exact:
//! - the **tension pitch-glide** — a hard hit stretches the head, so the pitch
//!   starts sharp and relaxes down over the first ~100 ms (that "boom" that
//!   falls in pitch); and
//! - **strike position** — hitting the centre favours the round axisymmetric
//!   modes (boomy), the edge favours the higher angular modes (slappy).
//!
//! A short filtered-noise **beater** transient sits on top. Reuses the shared
//! [`puget_dsp`] math; `no_std`, dependency-free.
//!
//! ```
//! use drum_core::Dohol;
//! let mut d = Dohol::new(48_000.0);
//! d.set_tune(72.0);        // boom pitch (Hz)
//! d.strike(0.9, 0.15);     // velocity, position (0 = centre .. 1 = rim)
//! let (l, r) = d.process();
//! # let _ = (l, r);
//! ```
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![cfg_attr(not(feature = "std"), no_std)]

use puget_dsp::mathf;

/// One membrane partial: its frequency ratio to the fundamental (0,1) mode, the
/// number of angular nodal lines `m` (0 = axisymmetric), its excitation gain,
/// and its decay time relative to the fundamental's.
#[derive(Clone, Copy)]
struct MembraneMode {
    ratio: f32,
    m: u8,
    gain: f32,
    decay_rel: f32,
}
const fn mode(ratio: f32, m: u8, gain: f32, decay_rel: f32) -> MembraneMode {
    MembraneMode { ratio, m, gain, decay_rel }
}

/// Ideal circular-membrane modes (Bessel zeros), air-loaded toward a real bass
/// drum: the higher modes are damped hard and decay fast, so the sound is a
/// low boom with a brief inharmonic wash — not a pitched timpano.
const MEMBRANE: &[MembraneMode] = &[
    // ratio     m   gain  decay_rel
    mode(1.000, 0, 1.00, 1.00), // (0,1) the boom
    mode(1.593, 1, 0.55, 0.45), // (1,1)
    mode(2.136, 2, 0.34, 0.32), // (2,1)
    mode(2.296, 0, 0.42, 0.50), // (0,2)
    mode(2.653, 3, 0.22, 0.24), // (3,1)
    mode(2.918, 1, 0.18, 0.22), // (1,2)
    mode(3.156, 4, 0.13, 0.17), // (4,1)
    mode(3.501, 2, 0.10, 0.15), // (2,2)
    mode(3.600, 0, 0.14, 0.28), // (0,3)
    mode(3.652, 5, 0.07, 0.12), // (5,1)
];

/// Tiny xorshift for the beater noise.
struct Rng(u32);
impl Rng {
    #[inline]
    fn next_f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Runtime state of one sounding mode: a decaying sinusoid whose frequency is
/// scaled every sample by the shared tension glide.
#[derive(Default, Clone, Copy)]
struct ModeState {
    phase: f32,
    inc: f32, // base per-sample phase increment (at the mode's steady freq)
    amp: f32,
    dec: f32, // per-sample amplitude multiplier
}

/// The **dohol**: a culturally-inspired membrane **bass drum**.
pub struct Dohol {
    fs: f32,
    fund: f32,
    modes: [ModeState; MEMBRANE.len()],
    // Tension pitch-glide: the per-sample frequency scale, relaxing to 1.
    glide: f32,
    glide_coef: f32,
    pitch_drop: f32,
    // Beater transient.
    rng: Rng,
    click_rem: u32,
    click_amp: f32,
    click_lp: f32,
    click_coef: f32, // beater hardness (one-pole cutoff)
    beater: f32,
    // Voicing.
    t60: f32,
    damp: f32,
    // Output shaping.
    out_lp: f32,
    out_pole: f32,
    drive: f32,
}

impl Dohol {
    pub fn new(fs: f32) -> Self {
        let mut d = Dohol {
            fs,
            fund: 72.0,
            modes: [ModeState::default(); MEMBRANE.len()],
            glide: 1.0,
            glide_coef: 0.0,
            pitch_drop: 0.18,
            rng: Rng(0x1234_5678),
            click_rem: 0,
            click_amp: 0.0,
            click_lp: 0.0,
            click_coef: 0.0,
            beater: 0.6,
            t60: 0.7,
            damp: 0.0,
            out_lp: 0.0,
            out_pole: 0.0,
            drive: 0.0,
        };
        d.set_beater(0.6);
        // Output roll-off ~3.5 kHz: a bass drum has little energy up top.
        d.out_pole = mathf::exp(-core::f32::consts::TAU * 3500.0 / fs);
        // Glide relaxes with ~45 ms time constant.
        d.glide_coef = 1.0 - mathf::exp(-1.0 / (0.045 * fs));
        d
    }

    /// Tune the boom — the fundamental (0,1) mode pitch, Hz. Bass drums live
    /// ~55–95 Hz; the dohol booms around 70.
    pub fn set_tune(&mut self, hz: f32) {
        self.fund = hz.clamp(20.0, 400.0);
    }

    /// Decay length of the boom in seconds (the fundamental's T60; higher modes
    /// scale down from it). Muffled heads → short; open resonant heads → long.
    pub fn set_decay(&mut self, seconds: f32) {
        self.t60 = seconds.clamp(0.05, 8.0);
    }

    /// Extra muffling (a hand or cloth on the head): 0 = open, 1 = heavily
    /// damped. Shortens every mode uniformly at strike time.
    pub fn set_muffle(&mut self, amount: f32) {
        self.damp = amount.clamp(0.0, 1.0);
    }

    /// Pitch-drop depth: how far sharp the head starts on a hit (0 = none, 1 =
    /// a big rubbery detune). The signature bass-drum "boom" that falls.
    pub fn set_pitch_drop(&mut self, amount: f32) {
        self.pitch_drop = amount.clamp(0.0, 1.0);
    }

    /// Beater hardness: 0 = a soft padded mallet (dull thud), 1 = a hard stick
    /// (bright crack). Sets the beater-noise low-pass cutoff.
    pub fn set_beater(&mut self, hardness: f32) {
        self.beater = hardness.clamp(0.0, 1.0);
        // ~350 Hz (soft) → ~7 kHz (hard).
        let fc = 350.0 + hardness.clamp(0.0, 1.0) * 6650.0;
        self.click_coef = 1.0 - mathf::exp(-core::f32::consts::TAU * fc / self.fs);
    }

    /// Output drive (soft-clip punch on hard hits): 0 = clean, 1 = fat.
    pub fn set_drive(&mut self, amount: f32) {
        self.drive = amount.clamp(0.0, 1.0);
    }

    /// Strike the head. `velocity` 0..1; `position` 0 = dead centre (roundest,
    /// boomiest) .. 1 = at the rim (more overtones, slappier, with rim crack).
    pub fn strike(&mut self, velocity: f32, position: f32) {
        let vel = velocity.clamp(0.0, 1.0);
        let pos = position.clamp(0.0, 1.0);
        // Head tension rises with a hard, central hit → a deeper initial glide.
        self.glide = 1.0 + self.pitch_drop * (0.5 + 0.5 * vel) * (1.0 - 0.5 * pos);
        let muffle = 1.0 - 0.85 * self.damp;
        for (st, md) in self.modes.iter_mut().zip(MEMBRANE.iter()) {
            let f = self.fund * md.ratio;
            if f >= self.fs * 0.48 {
                st.amp = 0.0;
                continue;
            }
            // Position weighting: centre feeds the axisymmetric (m=0) modes;
            // toward the rim the higher angular modes come alive.
            let m = md.m as f32;
            let pos_w = if md.m == 0 {
                // Axisymmetric modes: loudest dead centre, fading toward the rim.
                1.0 - 0.55 * pos
            } else {
                // Angular modes: quiet at centre, and the higher the angular
                // order the more the rim brings it out.
                (0.2 + pos) * (1.0 + 0.15 * m * pos)
            };
            st.inc = core::f32::consts::TAU * f / self.fs;
            // Slight phase scatter so repeated hits aren't identical.
            st.phase = (self.rng.next_f() + 1.0) * 0.5 * core::f32::consts::TAU;
            st.amp = vel * md.gain * pos_w;
            let t60 = (self.t60 * md.decay_rel * muffle).max(0.02);
            // Per-sample decay to reach -60 dB in t60 seconds.
            st.dec = mathf::exp(-6.9078 / (t60 * self.fs));
        }
        // Beater transient: a short filtered-noise crack. Harder/edge hits are
        // brighter and louder in the click.
        self.click_rem = (0.010 * self.fs) as u32;
        self.click_amp = vel * (0.25 + 0.55 * self.beater + 0.35 * pos);
    }

    /// Whether the drum is still sounding.
    pub fn active(&self) -> bool {
        self.click_rem > 0 || self.modes.iter().any(|m| m.amp > 1e-4)
    }

    /// One stereo sample (a single drum → centred/mono).
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        // Relax the tension glide toward 1.0.
        self.glide += (1.0 - self.glide) * self.glide_coef;

        // Sum the modal sinusoids.
        let mut s = 0.0;
        for st in &mut self.modes {
            if st.amp <= 1e-5 {
                continue;
            }
            s += st.amp * mathf::sin(st.phase);
            st.phase += st.inc * self.glide;
            if st.phase >= core::f32::consts::TAU {
                st.phase -= core::f32::consts::TAU;
            }
            st.amp *= st.dec;
        }

        // Beater transient.
        if self.click_rem > 0 {
            self.click_rem -= 1;
            let n = self.rng.next_f();
            self.click_lp += self.click_coef * (n - self.click_lp);
            s += self.click_amp * self.click_lp;
        }

        // Output roll-off (bass drum has little top) + optional soft-clip punch.
        self.out_lp += (1.0 - self.out_pole) * (s - self.out_lp);
        let mut out = self.out_lp;
        if self.drive > 0.0 {
            let k = 1.0 + 4.0 * self.drive;
            out = mathf::tanh(k * out) / mathf::tanh(k).max(1e-3);
        }
        out *= 1.1;
        (out, out)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn dohol_booms_and_decays() {
        let fs = 48_000.0;
        let mut d = Dohol::new(fs);
        d.set_tune(72.0);
        d.set_decay(0.7);
        d.strike(0.95, 0.12);
        let mut peak = 0.0f32;
        for i in 0..fs as usize {
            let (l, r) = d.process();
            assert!(l.is_finite() && r.is_finite(), "non-finite");
            if i < 4_000 {
                peak = peak.max(l.abs());
            }
        }
        assert!(peak > 0.02, "too quiet: {peak}");
        // Ring/decay out; a struck drum dies away.
        for _ in 0..fs as usize * 3 {
            d.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            tail = tail.max(d.process().0.abs());
        }
        assert!(tail < peak, "did not decay: tail {tail} vs peak {peak}");
    }

    #[test]
    fn boom_pitch_is_near_the_tuning() {
        // The dominant low partial should sit near the tuned fundamental once
        // the initial tension glide has relaxed.
        let fs = 48_000.0;
        let mut d = Dohol::new(fs);
        d.set_tune(70.0);
        d.set_decay(1.5);
        d.set_pitch_drop(0.0); // measure the steady pitch, not the glide
        d.strike(0.8, 0.1);
        // Skip the attack, capture a window.
        for _ in 0..2_000 {
            d.process();
        }
        let buf: Vec<f32> = (0..24_000).map(|_| d.process().0).collect();
        // Autocorrelation in the bass range 40–140 Hz.
        let (lo, hi) = ((fs / 140.0) as usize, (fs / 40.0) as usize);
        let mut best = f32::MIN;
        let mut lag0 = lo;
        for lag in lo..hi {
            let mut acc = 0.0;
            for i in 0..buf.len() - lag {
                acc += buf[i] * buf[i + lag];
            }
            if acc > best {
                best = acc;
                lag0 = lag;
            }
        }
        let f = fs / lag0 as f32;
        let cents = 1200.0 * (f / 70.0).log2();
        assert!(cents.abs() < 60.0, "boom off by {cents:.1} cents ({f:.1} Hz)");
    }

    #[test]
    fn edge_hit_is_brighter_than_centre() {
        // Hitting the rim should put more energy up the spectrum than a centre
        // hit (higher angular modes + a brighter beater click).
        fn high_ratio(pos: f32) -> f32 {
            let fs = 48_000.0;
            let mut d = Dohol::new(fs);
            d.set_tune(72.0);
            d.set_decay(0.6);
            d.strike(0.9, pos);
            // Crude HF vs total energy via a one-pole high-pass proxy.
            let mut prev = 0.0;
            let (mut hf, mut tot) = (0.0f32, 1e-9f32);
            for _ in 0..8_000 {
                let x = d.process().0;
                let h = x - prev;
                prev = x;
                hf += h * h;
                tot += x * x;
            }
            hf / tot
        }
        let centre = high_ratio(0.05);
        let edge = high_ratio(0.95);
        assert!(edge > centre, "edge {edge:.4} not brighter than centre {centre:.4}");
    }
}
