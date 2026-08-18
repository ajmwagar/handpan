//! # mallet-core
//!
//! Polyphonic modal synthesis for mallet percussion and bells — the Tier-1
//! proof that the handpan's modal engine is a reusable instrument core. Same
//! resonator-bank physics; different, physically-grounded mode data per
//! instrument (tuned bars, free-bar inharmonic sets, bell partials).
//!
//! ```
//! use mallet_core::{Mallet, Instrument};
//! let mut m = Mallet::new(48_000.0, Instrument::Marimba, 16);
//! m.strike_midi(60.0, 0.9);          // middle C
//! let (l, r) = m.process();
//! # let _ = (l, r);
//! ```
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

mod mathf;
mod resonator;

use resonator::Resonator;

/// One partial: frequency ratio to the fundamental, linear gain, decay×.
#[derive(Clone, Copy)]
pub struct ModeSpec {
    pub ratio: f32,
    pub gain: f32,
    pub decay: f32,
}

/// The character of one mallet/bell instrument.
pub struct Timbre {
    pub modes: &'static [ModeSpec],
    /// Fundamental −60 dB decay (s) at the C4 reference pitch.
    pub base_t60: f32,
    /// How much lower notes ring longer: t60 ∝ (C4/f)^decay_pitch.
    pub decay_pitch: f32,
    /// Excitation low-pass cutoff (Hz) — mallet hardness.
    pub mallet_cutoff: f32,
    pub attack_ms: f32,
    pub level: f32,
    /// Amplitude tremolo (0 = off) — the vibraphone's motor.
    pub tremolo_hz: f32,
    pub tremolo_depth: f32,
}

const fn m(ratio: f32, gain: f32, decay: f32) -> ModeSpec {
    ModeSpec { ratio, gain, decay }
}

// ── Mode data (physical / measured-informed) ────────────────────────────────

/// Rosewood bar, undercut so the overtones are two octaves + a third up.
/// Measured (Iowa MIS): tuned 1 : 4.00 : ~9.9, the 10× partial dies fast.
const MARIMBA: &[ModeSpec] = &[m(1.0, 1.0, 1.0), m(4.0, 0.22, 0.5), m(9.9, 0.07, 0.15)];
/// Rosewood bar tuned to the twelfth — brighter, shorter than marimba.
/// Measured (Iowa MIS): 3:1 exact; 3rd partial ~6.5; partials decay fast.
const XYLOPHONE: &[ModeSpec] =
    &[m(1.0, 1.0, 1.0), m(3.0, 0.3, 0.30), m(6.5, 0.12, 0.12), m(9.5, 0.05, 0.10)];
/// Aluminum bar — long metallic sustain, motor tremolo.
/// Measured (Iowa MIS): tuned dead-on 1 : 4.00 : 10.0; the 10× partial dies
/// fast while the fundamental rings for many seconds.
const VIBRAPHONE: &[ModeSpec] = &[m(1.0, 1.0, 1.0), m(4.0, 0.3, 0.6), m(10.0, 0.1, 0.1)];
/// Steel bar, natural free-bar inharmonic modes (1 : 2.756 : 5.404 : 8.933).
const GLOCKENSPIEL: &[ModeSpec] =
    &[m(1.0, 1.0, 1.0), m(2.756, 0.5, 0.6), m(5.404, 0.22, 0.4), m(8.933, 0.09, 0.25)];
/// Tubular bell — free-bar-like partials, the 2nd carries the strike pitch.
const TUBULAR_BELL: &[ModeSpec] =
    &[m(1.0, 0.7, 1.4), m(2.76, 1.0, 1.0), m(5.40, 0.5, 0.7), m(8.93, 0.2, 0.45)];
/// Church bell — the characteristic minor-third "tierce", with warble doublets.
const CHURCH_BELL: &[ModeSpec] = &[
    m(0.5, 0.5, 1.6),    // hum
    m(1.0, 1.0, 1.3),    // prime
    m(1.006, 0.6, 1.3),  // ...warble
    m(1.183, 0.8, 1.0),  // tierce (minor third)
    m(1.506, 0.5, 0.9),  // quint (fifth)
    m(2.0, 0.7, 0.8),    // nominal (octave)
    m(2.664, 0.3, 0.5),
    m(3.011, 0.25, 0.4),
];
/// Music-box comb tine — bright, thin, free-bar-ish, quick.
const MUSIC_BOX: &[ModeSpec] =
    &[m(1.0, 1.0, 1.0), m(2.76, 0.4, 0.5), m(5.4, 0.15, 0.3), m(8.9, 0.06, 0.2)];
/// Singing bowl — near-pure with a slow beating doublet, very long sustain.
const SINGING_BOWL: &[ModeSpec] = &[
    m(1.0, 0.55, 1.0),
    m(1.004, 0.5, 1.0), // beating pair
    m(2.7, 0.18, 0.7),
    m(2.712, 0.15, 0.7),
    m(5.2, 0.1, 0.5),
];

/// A named mallet/bell instrument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Instrument {
    Marimba,
    Xylophone,
    Vibraphone,
    Glockenspiel,
    TubularBell,
    ChurchBell,
    MusicBox,
    SingingBowl,
}

impl Instrument {
    pub fn timbre(self) -> Timbre {
        match self {
            Instrument::Marimba => Timbre { modes: MARIMBA, base_t60: 0.55, decay_pitch: 0.85, mallet_cutoff: 2200.0, attack_ms: 1.5, level: 1.0, tremolo_hz: 0.0, tremolo_depth: 0.0 },
            Instrument::Xylophone => Timbre { modes: XYLOPHONE, base_t60: 0.28, decay_pitch: 1.0, mallet_cutoff: 5500.0, attack_ms: 1.0, level: 1.0, tremolo_hz: 0.0, tremolo_depth: 0.0 },
            Instrument::Vibraphone => Timbre { modes: VIBRAPHONE, base_t60: 9.0, decay_pitch: 0.35, mallet_cutoff: 3000.0, attack_ms: 2.0, level: 1.0, tremolo_hz: 5.5, tremolo_depth: 0.3 },
            Instrument::Glockenspiel => Timbre { modes: GLOCKENSPIEL, base_t60: 1.3, decay_pitch: 0.5, mallet_cutoff: 8500.0, attack_ms: 0.8, level: 0.9, tremolo_hz: 0.0, tremolo_depth: 0.0 },
            Instrument::TubularBell => Timbre { modes: TUBULAR_BELL, base_t60: 3.5, decay_pitch: 0.8, mallet_cutoff: 4000.0, attack_ms: 2.5, level: 1.0, tremolo_hz: 0.0, tremolo_depth: 0.0 },
            Instrument::ChurchBell => Timbre { modes: CHURCH_BELL, base_t60: 5.0, decay_pitch: 0.9, mallet_cutoff: 3500.0, attack_ms: 3.0, level: 0.9, tremolo_hz: 0.0, tremolo_depth: 0.0 },
            Instrument::MusicBox => Timbre { modes: MUSIC_BOX, base_t60: 0.6, decay_pitch: 0.6, mallet_cutoff: 9000.0, attack_ms: 0.7, level: 0.7, tremolo_hz: 0.0, tremolo_depth: 0.0 },
            Instrument::SingingBowl => Timbre { modes: SINGING_BOWL, base_t60: 11.0, decay_pitch: 0.5, mallet_cutoff: 2500.0, attack_ms: 4.0, level: 1.0, tremolo_hz: 0.0, tremolo_depth: 0.0 },
        }
    }
}

const C4: f32 = 261.626;

// Tiny xorshift for strike noise.
#[derive(Clone)]
struct Rng(u32);
impl Rng {
    #[inline]
    fn bip(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

struct Voice {
    res: Vec<Resonator>,
    fs: f32,
    exc_rem: u32,
    exc_len: u32,
    exc_amp: f32,
    lp: f32,
    lp_a: f32,
    energy: f32,
    pan_l: f32,
    pan_r: f32,
    rng: Rng,
    active: bool,
}

impl Voice {
    fn new(n_modes: usize, fs: f32, seed: u32) -> Self {
        Voice {
            res: (0..n_modes).map(|_| Resonator::default()).collect(),
            fs,
            exc_rem: 0,
            exc_len: 1,
            exc_amp: 0.0,
            lp: 0.0,
            lp_a: 1.0,
            energy: 0.0,
            pan_l: 0.707,
            pan_r: 0.707,
            rng: Rng(seed | 1),
            active: false,
        }
    }

    fn strike(&mut self, freq: f32, vel: f32, t: &Timbre) {
        let t60 = t.base_t60 * mathf::powf(C4 / freq, t.decay_pitch);
        let t60 = t60.clamp(0.05, 20.0);
        for (i, mode) in t.modes.iter().enumerate() {
            let f = freq * mode.ratio;
            if f < self.fs * 0.49 {
                self.res[i].set(f, t60 * mode.decay, mode.gain * t.level, self.fs);
            } else {
                self.res[i].set(0.0, 0.05, 0.0, self.fs); // aliased mode → silent
            }
        }
        self.exc_len = ((t.attack_ms * 0.001) * self.fs) as u32;
        self.exc_len = self.exc_len.max(1);
        self.exc_rem = self.exc_len;
        self.exc_amp = vel.clamp(0.0, 1.0);
        self.lp = 0.0;
        self.lp_a = 1.0 - mathf::exp(-core::f32::consts::TAU * t.mallet_cutoff / self.fs);
        self.energy = vel;
        self.active = true;
        // Pan by pitch: low notes left, high notes right.
        let x = ((freq.log2() - 5.0) / 4.0).clamp(-0.7, 0.7); // ~C1..C9 → -0.7..0.7
        let theta = (x + 1.0) * 0.25 * core::f32::consts::PI;
        self.pan_l = mathf::cos(theta);
        self.pan_r = mathf::sin(theta);
    }

    #[inline]
    fn process(&mut self, damp: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        let raw = if self.exc_rem > 0 {
            let n = self.exc_len - self.exc_rem;
            let t = n as f32 / self.exc_len as f32;
            let env = mathf::exp(-6.0 * t);
            self.exc_rem -= 1;
            let click = if n == 0 { 1.0 } else { 0.0 };
            self.exc_amp * (env * 0.6 * self.rng.bip() + click)
        } else {
            0.0
        };
        self.lp += self.lp_a * (raw - self.lp);
        let exc = self.lp;
        let mut sum = 0.0;
        for r in &mut self.res {
            if damp < 0.9999 {
                r.damp_state(damp);
            }
            sum += r.process(exc);
        }
        // Track energy for voice stealing / deactivation.
        self.energy += 0.002 * (sum.abs() - self.energy);
        if self.exc_rem == 0 && self.energy < 2.0e-4 {
            self.active = false;
            for r in &mut self.res {
                r.reset();
            }
        }
        sum
    }
}

/// A polyphonic mallet/bell instrument.
pub struct Mallet {
    fs: f32,
    timbre: Timbre,
    voices: Vec<Voice>,
    trem_phase: f32,
    trem_hz: f32,
    trem_depth: f32,
    damp: f32,
    next: usize,
}

impl Mallet {
    /// Build with `polyphony` voices.
    pub fn new(fs: f32, instrument: Instrument, polyphony: usize) -> Self {
        let timbre = instrument.timbre();
        let n = timbre.modes.len();
        let voices = (0..polyphony.max(1))
            .map(|i| Voice::new(n, fs, 0x9E37_79B9 ^ (i as u32 + 1)))
            .collect();
        Mallet {
            fs,
            trem_hz: timbre.tremolo_hz,
            trem_depth: timbre.tremolo_depth,
            timbre,
            voices,
            trem_phase: 0.0,
            damp: 1.0,
            next: 0,
        }
    }

    /// Strike a pitch given in Hz.
    pub fn strike_hz(&mut self, freq: f32, velocity: f32) {
        // Prefer an inactive voice; else steal the quietest.
        let mut idx = None;
        let mut min_e = f32::INFINITY;
        let mut min_i = 0;
        for (i, v) in self.voices.iter().enumerate() {
            if !v.active {
                idx = Some(i);
                break;
            }
            if v.energy < min_e {
                min_e = v.energy;
                min_i = i;
            }
        }
        let i = idx.unwrap_or(min_i);
        self.voices[i].strike(freq, velocity, &self.timbre);
        self.next = i;
    }

    /// Strike a MIDI note number (A4 = 69 = 440 Hz).
    pub fn strike_midi(&mut self, note: f32, velocity: f32) {
        self.strike_hz(440.0 * mathf::powf(2.0, (note - 69.0) / 12.0), velocity);
    }

    /// Continuous damping (0 = ring, 1 = fast mute — a vibraphone pedal-up).
    pub fn set_damp(&mut self, amount: f32) {
        self.damp = 1.0 - amount.clamp(0.0, 1.0) * 0.004;
    }

    /// Override the tremolo (rate Hz, depth 0..1). 0 rate disables it.
    pub fn set_tremolo(&mut self, hz: f32, depth: f32) {
        self.trem_hz = hz.max(0.0);
        self.trem_depth = depth.clamp(0.0, 1.0);
    }

    /// Advance one stereo sample.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let mut l = 0.0;
        let mut r = 0.0;
        for v in &mut self.voices {
            let s = v.process(self.damp);
            l += s * v.pan_l;
            r += s * v.pan_r;
        }
        if self.trem_hz > 0.0 {
            self.trem_phase += core::f32::consts::TAU * self.trem_hz / self.fs;
            if self.trem_phase > core::f32::consts::TAU {
                self.trem_phase -= core::f32::consts::TAU;
            }
            let g = 1.0 - self.trem_depth * 0.5 * (1.0 - mathf::cos(self.trem_phase));
            l *= g;
            r *= g;
        }
        (l, r)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn strikes_are_stable_and_decay() {
        for inst in [
            Instrument::Marimba,
            Instrument::Vibraphone,
            Instrument::Glockenspiel,
            Instrument::ChurchBell,
            Instrument::SingingBowl,
        ] {
            let mut m = Mallet::new(48_000.0, inst, 8);
            m.strike_midi(60.0, 1.0);
            let mut peak = 0.0f32;
            for _ in 0..48_000 * 3 {
                let (l, r) = m.process();
                assert!(l.is_finite() && r.is_finite(), "{inst:?} non-finite");
                peak = peak.max(l.abs().max(r.abs()));
            }
            assert!(peak > 0.001, "{inst:?} silent");
            assert!(peak < 20.0, "{inst:?} runaway: {peak}");
        }
    }

    #[test]
    fn polyphony_steals_voices() {
        let mut m = Mallet::new(48_000.0, Instrument::Marimba, 4);
        for n in 60..80 {
            m.strike_midi(n as f32, 0.8);
            for _ in 0..500 {
                let (l, _) = m.process();
                assert!(l.is_finite());
            }
        }
    }
}
