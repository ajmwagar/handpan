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

use puget_dsp::mathf;
use puget_dsp::Ensemble;
mod resonator;

use resonator::{BandPass, Resonator};

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
    /// Tuned-tube resonance: dry/wet mix (0 = off) of a fundamental-tuned
    /// bandpass. Marimba/vibe bars sit over tubes that emphasise the
    /// fundamental and add a hollow "hoot"; kept subtle so it deepens the body
    /// without muddying the attack or shifting perceived pitch.
    pub tube_mix: f32,
    /// Quality (narrowness) of the tuned-tube bandpass.
    pub tube_q: f32,
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
            // base_t60 raised 0.55 → 0.90: measured marimba C4 fundamental T60
            // is ~1.4 s; the tuned tube adds the rest of the woody sustain.
            Instrument::Marimba => Timbre { modes: MARIMBA, base_t60: 0.90, decay_pitch: 0.85, mallet_cutoff: 2200.0, attack_ms: 1.5, level: 1.0, tremolo_hz: 0.0, tremolo_depth: 0.0, tube_mix: 0.20, tube_q: 2.5 },
            Instrument::Xylophone => Timbre { modes: XYLOPHONE, base_t60: 0.28, decay_pitch: 1.0, mallet_cutoff: 5500.0, attack_ms: 1.0, level: 1.0, tremolo_hz: 0.0, tremolo_depth: 0.0, tube_mix: 0.0, tube_q: 3.0 },
            Instrument::Vibraphone => Timbre { modes: VIBRAPHONE, base_t60: 9.0, decay_pitch: 0.35, mallet_cutoff: 3000.0, attack_ms: 2.0, level: 1.0, tremolo_hz: 5.5, tremolo_depth: 0.3, tube_mix: 0.15, tube_q: 3.5 },
            Instrument::Glockenspiel => Timbre { modes: GLOCKENSPIEL, base_t60: 1.3, decay_pitch: 0.5, mallet_cutoff: 8500.0, attack_ms: 0.8, level: 0.9, tremolo_hz: 0.0, tremolo_depth: 0.0, tube_mix: 0.0, tube_q: 3.0 },
            Instrument::TubularBell => Timbre { modes: TUBULAR_BELL, base_t60: 3.5, decay_pitch: 0.8, mallet_cutoff: 4000.0, attack_ms: 2.5, level: 1.0, tremolo_hz: 0.0, tremolo_depth: 0.0, tube_mix: 0.0, tube_q: 3.0 },
            Instrument::ChurchBell => Timbre { modes: CHURCH_BELL, base_t60: 5.0, decay_pitch: 0.9, mallet_cutoff: 3500.0, attack_ms: 3.0, level: 0.9, tremolo_hz: 0.0, tremolo_depth: 0.0, tube_mix: 0.0, tube_q: 3.0 },
            Instrument::MusicBox => Timbre { modes: MUSIC_BOX, base_t60: 0.6, decay_pitch: 0.6, mallet_cutoff: 9000.0, attack_ms: 0.7, level: 0.7, tremolo_hz: 0.0, tremolo_depth: 0.0, tube_mix: 0.0, tube_q: 3.0 },
            Instrument::SingingBowl => Timbre { modes: SINGING_BOWL, base_t60: 11.0, decay_pitch: 0.5, mallet_cutoff: 2500.0, attack_ms: 4.0, level: 1.0, tremolo_hz: 0.0, tremolo_depth: 0.0, tube_mix: 0.0, tube_q: 3.0 },
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
    /// Per-mode excitation drive weight — carries the velocity→brightness tilt
    /// (higher modes driven harder on a hard strike), like the handpan core.
    drive: Vec<f32>,
    fs: f32,
    exc_rem: u32,
    exc_len: u32,
    exc_amp: f32,
    lp: f32,
    lp_a: f32,
    tube: BandPass,
    tube_mix: f32,
    energy: f32,
    pan_l: f32,
    pan_r: f32,
    rng: Rng,
    active: bool,
}

/// How much a hard strike tilts the excitation toward the upper modes. A unit
/// strike raises the drive of a mode at ratio `r` by `K_BRIGHT·vel·(r−1)`, so a
/// hard hit injects more HF (a brighter spectrum), a soft hit stays mellow —
/// the same `exc_vel`-style term the handpan core uses.
const K_BRIGHT: f32 = 0.18;

impl Voice {
    fn new(n_modes: usize, fs: f32, seed: u32) -> Self {
        Voice {
            res: (0..n_modes).map(|_| Resonator::default()).collect(),
            drive: (0..n_modes).map(|_| 1.0).collect(),
            fs,
            exc_rem: 0,
            exc_len: 1,
            exc_amp: 0.0,
            lp: 0.0,
            lp_a: 1.0,
            tube: BandPass::default(),
            tube_mix: 0.0,
            energy: 0.0,
            pan_l: 0.707,
            pan_r: 0.707,
            rng: Rng(seed | 1),
            active: false,
        }
    }

    fn strike(&mut self, freq: f32, vel: f32, t: &Timbre) {
        let vel = vel.clamp(0.0, 1.0);
        let t60 = t.base_t60 * mathf::powf(C4 / freq, t.decay_pitch);
        let t60 = t60.clamp(0.05, 20.0);
        for (i, mode) in t.modes.iter().enumerate() {
            let f = freq * mode.ratio;
            if f < self.fs * 0.49 {
                self.res[i].set(f, t60 * mode.decay, mode.gain * t.level, self.fs);
                // Velocity→brightness: drive the upper modes harder on a hard
                // strike. Fundamental (ratio 1) is unchanged.
                self.drive[i] = 1.0 + K_BRIGHT * vel * (mode.ratio - 1.0);
            } else {
                self.res[i].set(0.0, 0.05, 0.0, self.fs); // aliased mode → silent
                self.drive[i] = 1.0;
            }
        }
        // Tuned-tube resonance follows the struck fundamental (subtle body).
        self.tube_mix = t.tube_mix;
        if self.tube_mix > 0.0 {
            self.tube.set(freq, t.tube_q, self.fs);
            self.tube.reset();
        }
        self.exc_len = ((t.attack_ms * 0.001) * self.fs) as u32;
        self.exc_len = self.exc_len.max(1);
        self.exc_rem = self.exc_len;
        self.exc_amp = vel;
        self.lp = 0.0;
        // Velocity also opens the excitation low-pass: a harder strike is a
        // harder mallet contact, passing more of the click's HF (0.55×..1.15×
        // the nominal mallet cutoff across the dynamic range).
        let cutoff = t.mallet_cutoff * (0.55 + 0.6 * vel);
        self.lp_a = 1.0 - mathf::exp(-core::f32::consts::TAU * cutoff / self.fs);
        self.energy = vel;
        self.active = true;
        // Pan by pitch: low notes left, high notes right.
        let x = ((mathf::log2(freq) - 5.0) / 4.0).clamp(-0.7, 0.7); // ~C1..C9 → -0.7..0.7
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
        for (i, r) in self.res.iter_mut().enumerate() {
            if damp < 0.9999 {
                r.damp_state(damp);
            }
            // Per-mode velocity brightness tilt (upper modes driven harder).
            sum += r.process(exc * self.drive[i]);
        }
        // Tuned-tube resonance: a gentle fundamental-tuned bandpass mixed in to
        // deepen the body (marimba/vibe). Wet path peaks at unity, so it only
        // colours — no runaway — and stays subtle at `tube_mix`.
        if self.tube_mix > 0.0 {
            sum += self.tube_mix * self.tube.process(sum);
        }
        // Track energy for voice stealing / deactivation.
        self.energy += 0.002 * (sum.abs() - self.energy);
        if self.exc_rem == 0 && self.energy < 2.0e-4 {
            self.active = false;
            for r in &mut self.res {
                r.reset();
            }
            self.tube.reset();
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

/// One player in a mallet section: a [`Mallet`] plus a per-player velocity
/// spread. Mono-summed so the shared [`Ensemble`] can place it in the field.
struct MalletVoice {
    mallet: Mallet,
    vel_scale: f32,
}
impl puget_dsp::Voice for MalletVoice {
    type Event = f32; // strike velocity
    fn humanize(&mut self, seed: u32) {
        // Velocity spread (harder/softer players); tuning spread + flam
        // decorrelate the rest, so no resonator re-seed is needed.
        let h = ((seed & 0xffff) as f32 / 65_535.0) * 2.0 - 1.0;
        self.vel_scale = 0.88 + 0.12 * h;
    }
    fn trigger(&mut self, freq: f32, velocity: f32) {
        self.mallet.strike_hz(freq, (velocity * self.vel_scale).clamp(0.0, 1.0));
    }
    fn release(&mut self) {} // struck — rings down on its own
    fn process(&mut self) -> f32 {
        let (l, r) = self.mallet.process();
        (l + r) * 0.5
    }
}

/// A **mallet section**: humanized players striking together (the shared
/// [`Ensemble`]). Struck sections get their feel from timing flam, velocity
/// spread, tuning spread and stereo placement. The chair count is the "how many
/// players" control; `set_spread`/`set_width` are the tuning/image macros.
pub struct MalletEnsemble {
    section: Ensemble<MalletVoice>,
}

impl MalletEnsemble {
    /// A section of up to `max_chairs` players (each with `poly` voices),
    /// `spread_cents` tuning spread.
    pub fn new(
        fs: f32,
        instrument: Instrument,
        max_chairs: usize,
        spread_cents: f32,
        poly: usize,
    ) -> Self {
        // Flam: strikes don't land together — up to ~25 ms of onset spread.
        let section = Ensemble::new(fs, max_chairs, spread_cents, 25.0, |_| MalletVoice {
            mallet: Mallet::new(fs, instrument, poly.max(1)),
            vel_scale: 1.0,
        });
        MalletEnsemble { section }
    }

    /// How many players are sounding (the "chairs" encoder), clamped `[1, max]`.
    pub fn set_chairs(&mut self, n: usize) {
        self.section.set_active(n);
    }
    /// Number of active chairs.
    pub fn chairs(&self) -> usize {
        self.section.active()
    }
    /// Tuning spread across the section, in cents (intimate ↔ wide).
    pub fn set_spread(&mut self, cents: f32) {
        self.section.set_spread(cents);
    }
    /// Stereo width, 0..1 (mono ↔ full field).
    pub fn set_width(&mut self, width: f32) {
        self.section.set_width(width);
    }
    /// Strike a pitch (Hz) across the section.
    pub fn strike_hz(&mut self, freq: f32, velocity: f32) {
        self.section.trigger(freq, velocity);
    }
    /// Strike a MIDI note across the section.
    pub fn strike_midi(&mut self, note: f32, velocity: f32) {
        self.strike_hz(440.0 * mathf::powf(2.0, (note - 69.0) / 12.0), velocity);
    }
    /// Damping (0 = ring, 1 = fast mute), section-wide.
    pub fn set_damp(&mut self, amount: f32) {
        self.section.for_each_voice(|v, _| v.mallet.set_damp(amount));
    }
    /// Tremolo (rate Hz, depth), section-wide.
    pub fn set_tremolo(&mut self, hz: f32, depth: f32) {
        self.section.for_each_voice(|v, _| v.mallet.set_tremolo(hz, depth));
    }
    /// One stereo sample of the whole section.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        self.section.process()
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn mallet_section_stacks_and_is_stereo() {
        let fs = 48_000.0;
        let mut sec = MalletEnsemble::new(fs, Instrument::Marimba, 8, 5.0, 4);
        sec.set_chairs(6);
        assert_eq!(sec.chairs(), 6);
        sec.strike_midi(60.0, 1.0);
        let (mut pl, mut pr, mut width) = (0.0f32, 0.0f32, 0.0f32);
        for i in 0..48_000 {
            let (l, r) = sec.process();
            assert!(l.is_finite() && r.is_finite());
            if i < 24_000 {
                pl = pl.max(l.abs());
                pr = pr.max(r.abs());
                width = width.max((l - r).abs());
            }
        }
        assert!(pl > 0.02 && pr > 0.02, "section silent: {pl} {pr}");
        assert!(width > 0.01, "section not spread/stereo: {width}");
        // 1 chair is a thin solo (one voice), not the fat section.
        let mut solo = MalletEnsemble::new(fs, Instrument::Marimba, 8, 5.0, 4);
        solo.set_chairs(1);
        solo.strike_midi(60.0, 1.0);
        let mut mono_w = 0.0f32;
        for _ in 0..24_000 {
            let (l, r) = solo.process();
            mono_w = mono_w.max((l - r).abs());
        }
        assert!(mono_w < width, "1 chair should be narrower than 6");
    }

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

    /// Spectral centroid (energy-weighted mean frequency) of a render, via a
    /// coarse DFT over a fixed frequency grid.
    fn centroid(buf: &[f32], fs: f32) -> f32 {
        let (fmin, fmax, nb) = (20.0f32, 8000.0f32, 400usize);
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for b in 0..nb {
            let f = fmin + (fmax - fmin) * (b as f32 / (nb - 1) as f32);
            let w = core::f32::consts::TAU * f / fs;
            let (mut re, mut im) = (0.0f64, 0.0f64);
            // Decimate by 4 for speed — plenty for a centroid.
            let mut i = 0;
            while i < buf.len() {
                let s = buf[i] as f64;
                let ph = w as f64 * i as f64;
                re += s * libm_cos(ph);
                im += s * libm_sin(ph);
                i += 4;
            }
            let mag = (re * re + im * im).sqrt();
            num += f as f64 * mag;
            den += mag;
        }
        if den > 0.0 {
            (num / den) as f32
        } else {
            0.0
        }
    }

    // std is available in this test cfg, so use it directly for the analysis.
    fn libm_cos(x: f64) -> f64 {
        x.cos()
    }
    fn libm_sin(x: f64) -> f64 {
        x.sin()
    }

    fn strike_centroid(inst: Instrument, midi: f32, vel: f32, fs: f32) -> f32 {
        let mut m = Mallet::new(fs, inst, 4);
        m.strike_midi(midi, vel);
        let n = (0.3 * fs) as usize;
        let mut buf = Vec::with_capacity(n);
        for _ in 0..n {
            let (l, r) = m.process();
            buf.push((l + r) * 0.5);
        }
        centroid(&buf, fs)
    }

    /// A harder strike must be *brighter*, not just louder: the spectral
    /// centroid has to rise monotonically with strike velocity. Regression
    /// guard for the velocity→brightness excitation tilt.
    #[test]
    fn harder_strike_is_brighter() {
        let fs = 48_000.0;
        for inst in [Instrument::Marimba, Instrument::Vibraphone, Instrument::Glockenspiel] {
            let vels = [0.2f32, 0.5, 0.8, 1.0];
            let cs: Vec<f32> = vels.iter().map(|&v| strike_centroid(inst, 60.0, v, fs)).collect();
            // Monotonic rise across the dynamic range.
            for w in cs.windows(2) {
                assert!(
                    w[1] > w[0] + 1.0,
                    "{inst:?} centroid not rising with velocity: {cs:?}"
                );
            }
            // And a meaningfully brighter top vs. bottom (not a rounding wiggle).
            assert!(
                cs[3] > cs[0] * 1.05,
                "{inst:?} centroid barely moves 0.2→1.0: {cs:?}"
            );
        }
    }

    /// Pitch must survive the brightness/tube changes: autocorrelation on a
    /// marimba C4 strike should still land on ~261.6 Hz.
    #[test]
    fn pitch_unchanged_by_polish() {
        let fs = 48_000.0;
        let mut m = Mallet::new(fs, Instrument::Marimba, 4);
        m.strike_midi(60.0, 0.9);
        let n = (0.5 * fs) as usize;
        let mut buf = Vec::with_capacity(n);
        for _ in 0..n {
            let (l, r) = m.process();
            buf.push((l + r) * 0.5);
        }
        let (fmin, fmax) = (150.0f32, 400.0f32);
        let lag_min = (fs / fmax) as usize;
        let lag_max = (fs / fmin) as usize;
        let (mut best, mut best_lag) = (0.0f64, lag_min);
        for lag in lag_min..lag_max.min(buf.len() - 1) {
            let mut s = 0.0f64;
            for i in 0..(buf.len() - lag) {
                s += buf[i] as f64 * buf[i + lag] as f64;
            }
            if s > best {
                best = s;
                best_lag = lag;
            }
        }
        let f0 = fs / best_lag as f32;
        assert!((f0 - 261.626).abs() < 6.0, "marimba pitch drifted: {f0} Hz");
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
