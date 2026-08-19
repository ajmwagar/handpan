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

use puget_dsp::mathf;
use puget_dsp::{Ensemble, Voice};

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
        let fl = mathf::floor(dr);
        let i0 = fl as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = dr - fl;
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
    /// Air jet on an open–open bore — all harmonics, breathy.
    Flute,
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
    lip_x1: f32,
    lip_x2: f32,
    bright: f32,
    // Brass bell + lip drive: negative-resistance valve gain, lip/note ratio,
    // lip pole radius (Q), tuning trim, wave-steepening amount, bell radiation
    // mix, bell one-pole state, stored bore delay, and a slow DC tracker so the
    // brassiness steepening keeps all harmonics without pulling pitch.
    lip_drive: f32,
    lip_factor: f32,
    lip_r: f32,
    tune_trim: f32,
    brass: f32,
    bell_mix: f32,
    bell_y1: f32,
    bore_delay: f32,
    brass_dc: f32,

    // Flute (air-jet) state: `jet` is the embouchure air-jet delay line (length
    // = jet_ratio·bore); `jdc_*` DC-blocks the reflected bore pressure; the
    // tuning constants correct the jet-drive pitch; the breath bias/scale pin
    // the flute's narrow blowing window.
    jet: Delay,
    jet_ratio: f32,
    jet_refl: f32,
    end_refl: f32,
    jdc_x1: f32,
    jdc_y1: f32,
    tune_scale: f32,
    tune_off: f32,
    flute_breath_bias: f32,
    flute_breath_scale: f32,

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

    // Always-on organic breath movement: two incommensurate slow LFOs + a slow
    // random walk, so sustained notes breathe instead of sitting dead-static.
    lfo_a: f32,
    lfo_b: f32,
    breath_walk: f32,
    breath_move: f32,
    // Breath-modulation routing. `breath_cv` is an external modulation source
    // (a bipolar CV, ~[-1, 1]) NORMALLED to the onboard LFO: when `Some`, it
    // drives the breath movement; when `None`, the internal LFO does. `depth`
    // scales either source (a front-panel attenuator).
    breath_cv: Option<f32>,
    breath_mod_depth: f32,

    // Bell-radiation even-harmonic term (DC-tracked) + output DC blocker.
    even_dc: f32,
    dc_x1: f32,
    dc_y1: f32,

    // Output radiation low-pass (two cascaded one-poles = 12 dB/oct): the steep
    // upper-harmonic roll-off a real bore/bell has. `out_pole` closer to 1 = darker.
    out_pole: f32,
    out_lp: f32,
    out_lp2: f32,

    out_gain: f32,
}

impl Wind {
    pub fn new(fs: f32, kind: WindKind) -> Self {
        let max = (fs / 30.0) as usize + 4; // lowest ~30 Hz bore
        let (reed_offset, reed_slope, out_gain, noise_gain) = match kind {
            // Clarinet reed: classic STK reed table (offset 0.7, slope -0.44).
            WindKind::Clarinet => (0.7, -0.44, 1.0, 0.07),
            // Flute: no reed table (jet drive); strong airy breath noise (a
            // real flute is markedly breathier than the raw jet oscillation).
            WindKind::Flute => (0.0, 0.0, 1.0, 0.14),
            // Saxophone reed: the clarinet's stable single-reed loop (strong
            // fundamental, no octave overblow) voiced as a sax via strong
            // even-harmonic radiation + a reedy formant. Less breath noise so it
            // reads focused rather than washed out.
            WindKind::Saxophone => (0.7, -0.50, 1.6, 0.05),
            // Trumpet uses the lip resonator, not the reed table; noise seeds
            // a natural onset.
            WindKind::Trumpet => (0.0, 0.0, 2.0, 0.03),
        };
        // Reflection: clarinet inverts (odd-harmonic, quarter-wave); the
        // conical sax and the brass bore are effectively open (all harmonics),
        // so their loop does not invert. `Reflect::tick` applies `-loss`, so a
        // negative `loss` yields a non-inverting (all-harmonic) loop.
        let (refl_loss, refl_damp) = match kind {
            WindKind::Clarinet => (0.95, 0.81),
            // Flute: inverting loss LP; heavy damping (0.60) keeps the 1st
            // register (lighter overblows to the octave or squeals).
            WindKind::Flute => (0.95, 0.60),
            // Sax: the clarinet's INVERTING single-reed loop — stable and
            // fundamental-strong (no octave overblow); brighter/reedier.
            WindKind::Saxophone => (0.95, 0.4),
            // Trumpet: loss = low-freq bell reflection gain; damp = the bell
            // one-pole low-pass coeff (~1.2 kHz cutoff at bright 0.5).
            WindKind::Trumpet => (0.99, 0.82),
        };
        let radiate = match kind {
            // Clarinet: presence lift restoring the sub-cutoff harmonics + a
            // ~3 kHz formant (fit to Iowa MIS spectra).
            WindKind::Clarinet => {
                [Peaking::new(1000.0, 0.6, 13.0, fs), Peaking::new(3000.0, 1.2, 5.0, fs)]
            }
            // Flute: a gentle low lift + a soft high cut (breathy, rounded).
            WindKind::Flute => {
                [Peaking::new(500.0, 0.7, 3.0, fs), Peaking::new(4000.0, 0.7, -6.0, fs)]
            }
            // Saxophone: a low lift for body + the reedy "honk" formant
            // (~1.5 kHz) that gives the sax its focus/presence.
            WindKind::Saxophone => {
                [Peaking::new(700.0, 0.7, 3.0, fs), Peaking::new(1500.0, 1.4, 8.0, fs)]
            }
            // Trumpet: the brass formant ("bridge") sits ~1.2–1.5 kHz — that is
            // where the real Iowa trumpet peaks (its h4), not up at 2.4 kHz.
            WindKind::Trumpet => {
                [Peaking::new(1200.0, 1.0, 11.0, fs), Peaking::new(4500.0, 0.7, -4.0, fs)]
            }
        };
        // Flute jet parameters (jet_ratio, jet_refl, end_refl, tune_scale,
        // tune_off) and its breath window (bias, scale). tune_scale corrects the
        // jet-drive pitch (empirical for jet_ratio 0.30 + damp 0.60).
        let (jet_ratio, jet_refl, end_refl, tune_scale, tune_off) = match kind {
            WindKind::Flute => (0.30, 0.5, 0.5, 1.542, 0.0),
            _ => (0.0, 0.0, 0.0, 1.0, 0.0),
        };
        let (flute_breath_bias, flute_breath_scale) = (0.87, 0.07);
        Wind {
            fs,
            kind,
            bore: Delay::new(max),
            refl: Reflect::new(refl_loss, refl_damp),
            radiate,
            freq: 220.0,
            reed_offset,
            reed_slope,
            jet: Delay::new(max),
            jet_ratio,
            jet_refl,
            end_refl,
            jdc_x1: 0.0,
            jdc_y1: 0.0,
            tune_scale,
            tune_off,
            flute_breath_bias,
            flute_breath_scale,
            lip_a1: 0.0,
            lip_a2: 0.0,
            lip_b0: 0.0,
            lip_y1: 0.0,
            lip_y2: 0.0,
            lip_x1: 0.0,
            lip_x2: 0.0,
            bright: 0.5,
            lip_drive: -0.7,
            lip_factor: 1.0,
            lip_r: 0.98,
            tune_trim: -1.0,
            brass: if matches!(kind, WindKind::Trumpet) { 90.0 } else { 0.0 },
            bell_mix: 0.92,
            bell_y1: 0.0,
            bore_delay: 100.0,
            brass_dc: 0.0,
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
            lfo_a: 0.0,
            lfo_b: 1.7,
            breath_walk: 0.0,
            breath_move: 1.0,
            breath_cv: None,
            breath_mod_depth: 0.045,
            even_dc: 0.0,
            dc_x1: 0.0,
            dc_y1: 0.0,
            // Radiation low-pass cutoff: clarinet ~9 kHz (near-transparent, the
            // loop already shapes it), sax ~600 Hz (steep roll-off → strong
            // fundamental like the real alto), trumpet ~1.9 kHz.
            out_pole: {
                let fc = match kind {
                    // Near-transparent: the flute/clarinet loops already shape
                    // the top (the flute also has a radiation high-cut shelf).
                    WindKind::Clarinet | WindKind::Flute => 9000.0,
                    // Post-loop, so it darkens timbre without risking the
                    // oscillation: a steep roll-off gives the sax its
                    // fundamental-dominant body and rolls the trumpet's top off.
                    WindKind::Saxophone => 3500.0,
                    WindKind::Trumpet => 3000.0,
                };
                mathf::exp(-core::f32::consts::TAU * fc / fs)
            },
            out_lp: 0.0,
            out_lp2: 0.0,
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
            // Open–open jet bore: full period, scaled/offset to hit pitch.
            WindKind::Flute => self.fs / self.freq * self.tune_scale - self.tune_off - 1.0 - fgd,
            // Sax on the clarinet's quarter-wave loop (half an open pipe); the
            // even harmonics are added at the output, not by the bore.
            WindKind::Saxophone => self.fs / self.freq * 0.5 - 1.0 - fgd,
            // Brass bore: non-inverting all-harmonic loop; the tune_trim absorbs
            // the cache term and the bell-filter delay.
            WindKind::Trumpet => self.fs / self.freq - fgd + self.tune_trim,
        };
        let d = d.max(4.0);
        self.bore_delay = d;
        self.bore.set_delay(d);
        if self.kind == WindKind::Flute {
            self.jet.set_delay((d * self.jet_ratio).max(1.0));
        }
        if self.kind == WindKind::Trumpet {
            self.tune_lip(self.freq);
        }
    }

    /// Tune the trumpet's lip resonator (a normalized band-pass, peak gain 1) to
    /// a buzzing frequency near the note.
    fn tune_lip(&mut self, freq: f32) {
        let lf = freq * self.lip_factor;
        let w = core::f32::consts::TAU * lf / self.fs;
        let r = self.lip_r;
        self.lip_a1 = 2.0 * r * mathf::cos(w);
        self.lip_a2 = -(r * r);
        self.lip_b0 = 0.5 - 0.5 * r * r; // normalized band-pass (peak = 1)
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
            // The flute's blowing window is narrow (onset ~0.84; overblows > 1.0).
            WindKind::Flute => self.flute_breath_bias + self.flute_breath_scale * b,
            WindKind::Saxophone => 0.32 + 0.34 * b,
            WindKind::Trumpet => 0.30 + 0.45 * b,
        };
    }

    /// Decorrelate this voice from other copies of the same instrument, so a
    /// stacked section sounds like many players rather than one loud unison:
    /// re-seeds the noise, and desyncs the breath LFOs and vibrato phase/rate.
    pub fn humanize(&mut self, seed: u32) {
        self.rng ^= seed.wrapping_mul(0x9E37_79B9) | 1;
        let f = ((seed & 0xffff) as f32) / 65_535.0; // 0..1
        self.lfo_a = f * core::f32::consts::TAU;
        self.lfo_b = (1.0 - f) * core::f32::consts::TAU;
        self.vib_phase = f * core::f32::consts::TAU;
        self.vib_rate = 5.0 * (0.90 + 0.20 * f); // ±10% vibrato-rate spread
        self.breath_walk = 0.0;
    }

    /// External breath-modulation CV, **normalled to the onboard LFO**.
    ///
    /// Pass `Some(cv)` — a bipolar signal, nominally `[-1, 1]` — to drive the
    /// breath movement from an external source (a CV jack, host automation, an
    /// MPE dimension). Feed the current value each sample/block while the input
    /// is "patched". Pass `None` to un-patch: the internal LFO takes over again,
    /// exactly like a normalled Eurorack jack. The LFO keeps running underneath
    /// either way, so switching back is seamless.
    pub fn set_breath_mod(&mut self, cv: Option<f32>) {
        self.breath_cv = cv;
    }

    /// Depth of the breath modulation (attenuator on whichever source is active,
    /// external or the LFO). 0 = none, ~0.045 = the subtle default sway, up to a
    /// deep tremolo. Applies to both the CV input and the onboard LFO.
    pub fn set_breath_mod_depth(&mut self, depth: f32) {
        self.breath_mod_depth = depth.clamp(0.0, 0.6);
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
            // Narrow swing around 0.60 — a wide swing destabilizes the jet.
            WindKind::Flute => 0.64 - 0.08 * a,
            // Reedy: brighter than the clarinet, around the honk formant.
            WindKind::Saxophone => 0.50 - 0.25 * a,
            // Moves the bell cutoff (brighter = higher cutoff = smaller g).
            WindKind::Trumpet => 0.87 - 0.10 * a,
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

        // Organic breath movement: two slow incommensurate LFOs (~0.7 & ~2.3 Hz)
        // plus a slow random walk — the constant micro-fluctuation of a real
        // player's air. Always on; keeps sustained notes alive.
        self.lfo_a += core::f32::consts::TAU * 0.7 / self.fs;
        self.lfo_b += core::f32::consts::TAU * 2.3 / self.fs;
        if self.lfo_a > core::f32::consts::TAU {
            self.lfo_a -= core::f32::consts::TAU;
        }
        if self.lfo_b > core::f32::consts::TAU {
            self.lfo_b -= core::f32::consts::TAU;
        }
        self.breath_walk += 0.0002 * (self.white() - self.breath_walk);
        let lfo = 0.55 * mathf::sin(self.lfo_a) + 0.30 * mathf::sin(self.lfo_b)
            + 3.0 * self.breath_walk;
        // Normalled routing: the external breath-mod CV when patched, otherwise
        // the onboard LFO. Smoothed so it's a gentle sway, not a warble.
        let drift = self.breath_cv.unwrap_or(lfo);
        self.breath_move += 0.02 * ((1.0 + self.breath_mod_depth * drift) - self.breath_move);

        // Breath = envelope + turbulence noise + vibrato (as pressure ripple).
        // The noise is low-passed into an airy "breath" band rather than hiss.
        let n = self.white();
        self.noise_lp += 0.2 * (n - self.noise_lp);
        let mut breath = self.breath_env * self.breath_move;
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
            // Air jet (flute): the reflected bore pressure (DC-blocked) drives a
            // cubic jet nonlinearity through the embouchure delay, summed with
            // the end reflection back into the open bore. All harmonics.
            WindKind::Flute => {
                let filt = self.refl.tick(self.bore.last_out());
                let temp = filt - self.jdc_x1 + 0.995 * self.jdc_y1;
                self.jdc_x1 = filt;
                self.jdc_y1 = temp;
                let mut pd = breath - self.jet_refl * temp;
                pd = self.jet.tick(pd);
                let jet = (pd * (pd * pd - 1.0)).clamp(-1.0, 1.0);
                0.3 * self.bore.tick(jet + self.end_refl * temp)
            }
            // Lip reed + flared bell (trumpet). The bell is a real
            // frequency-dependent reflection: a one-pole low-pass reflects the
            // lows back into the bore (forming the standing waves) while the
            // highs radiate out — so the emitted sound is the high-pass
            // *transmission*, which is why brass has a weak fundamental and a
            // strong bright mid-harmonic cluster.
            WindKind::Trumpet => {
                let bore_out = self.bore.last_out();
                // Flared-bell one-pole low-pass at the bell cutoff (~1.2 kHz).
                let g = self.refl.damp;
                self.bell_y1 = (1.0 - g) * bore_out + g * self.bell_y1;
                let lp = self.bell_y1;
                // Reflection that stays in the bore (non-inverting → all harmonics).
                let reflected = self.refl.loss * lp;
                // Lip resonator: normalized band-pass of the pressure across the lips.
                let delta = breath - reflected;
                let lip = self.lip_b0 * (delta - self.lip_x2)
                    + self.lip_a1 * self.lip_y1
                    + self.lip_a2 * self.lip_y2;
                self.lip_x2 = self.lip_x1;
                self.lip_x1 = delta;
                self.lip_y2 = self.lip_y1;
                self.lip_y1 = lip;
                // One-way lip valve; drive is NEGATIVE (negative resistance → sustains).
                let area = (self.lip_drive * lip).clamp(0.0, 1.0);
                let inj = reflected + area * delta;
                // Brassiness = wave steepening: high-pressure fronts travel
                // faster → the bore delay shortens with pressure. DC-blocked so
                // the mean delay (pitch) is fixed while harmonics brighten.
                let bl = (0.6 + 0.4 * self.bright) * self.breath_env;
                self.brass_dc += 0.001 * (bore_out - self.brass_dc);
                let dmod = (self.brass * bl * (bore_out - self.brass_dc)).clamp(-3.0, 3.0);
                self.bore.set_delay(self.bore_delay - dmod);
                self.bore.tick(inj);
                // Radiated output = bell transmission (high-shelf complement).
                bore_out - self.bell_mix * lp
            }
        };

        // Squared (even-harmonic) radiation term. The clarinet/sax run on an
        // odd-only cylindrical loop, so their even harmonics come from here —
        // small for the clarinet, strong for the sax (that's what turns the
        // hollow reed tone into a full, all-harmonic saxophone).
        let even_amt = match self.kind {
            WindKind::Clarinet => 0.09,
            WindKind::Saxophone => 1.4,
            _ => 0.0,
        };
        if even_amt > 0.0 {
            let sq = out * out;
            self.even_dc += 0.0008 * (sq - self.even_dc);
            out += even_amt * (sq - self.even_dc);
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
        // Radiation low-pass (two cascaded one-poles): the bore/bell's steep
        // upper-harmonic roll-off.
        self.out_lp = (1.0 - self.out_pole) * r + self.out_pole * self.out_lp;
        self.out_lp2 = (1.0 - self.out_pole) * self.out_lp + self.out_pole * self.out_lp2;
        self.out_lp2 * self.out_gain
    }
}

impl Voice for Wind {
    type Event = f32; // breath pressure
    fn humanize(&mut self, seed: u32) {
        Wind::humanize(self, seed);
    }
    fn trigger(&mut self, freq: f32, breath: f32) {
        self.note_on(freq, breath);
    }
    fn release(&mut self) {
        self.note_off();
    }
    fn process(&mut self) -> f32 {
        Wind::process(self)
    }
}

/// A **section** of one wind instrument: humanized copies stacked and spread
/// across the stereo field (the shared [`Ensemble`]). The chair count is the
/// "how many players" control; `set_spread`/`set_width` are the tuning and
/// stereo-image macros; breath/vibrato/brightness fan out to every chair.
pub struct WindEnsemble {
    section: Ensemble<Wind>,
}

impl WindEnsemble {
    /// A section of up to `max_chairs` players, `spread_cents` tuning spread.
    pub fn new(fs: f32, kind: WindKind, max_chairs: usize, spread_cents: f32) -> Self {
        // Up to ~20 ms of onset stagger so attacks aren't perfectly synced.
        let section = Ensemble::new(fs, max_chairs, spread_cents, 20.0, |_| Wind::new(fs, kind));
        WindEnsemble { section }
    }

    /// How many players are sounding (the "chairs" encoder), clamped `[1, max]`.
    pub fn set_chairs(&mut self, n: usize) {
        self.section.set_active(n);
    }
    /// Number of active chairs.
    pub fn chairs(&self) -> usize {
        self.section.active()
    }
    /// Detune spread across the section, in cents (intimate ↔ wide).
    pub fn set_spread(&mut self, cents: f32) {
        self.section.set_spread(cents);
    }
    /// Stereo width, 0..1 (mono ↔ full field).
    pub fn set_width(&mut self, width: f32) {
        self.section.set_width(width);
    }
    /// Start a note across the section.
    pub fn note_on(&mut self, freq: f32, breath: f32) {
        self.section.trigger(freq, breath);
    }
    /// Release the section.
    pub fn note_off(&mut self) {
        self.section.release();
    }
    /// Vibrato — depth shared, rate spread per chair.
    pub fn set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        self.section.for_each_voice(|v, s| v.set_vibrato(rate_hz * s, depth));
    }
    /// Brightness, section-wide.
    pub fn set_brightness(&mut self, amount: f32) {
        self.section.for_each_voice(|v, _| v.set_brightness(amount));
    }
    /// Continuous breath, section-wide.
    pub fn set_breath(&mut self, breath: f32) {
        self.section.for_each_voice(|v, _| v.set_breath(breath));
    }
    /// Breath-mod CV (normalled to each chair's LFO), section-wide.
    pub fn set_breath_mod(&mut self, cv: Option<f32>) {
        self.section.for_each_voice(|v, _| v.set_breath_mod(cv));
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
        for kind in
            [WindKind::Clarinet, WindKind::Flute, WindKind::Saxophone, WindKind::Trumpet]
        {
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
    fn ensemble_stacks_chairs_and_is_stereo() {
        let fs = 48_000.0;
        let mut sec = WindEnsemble::new(fs, WindKind::Clarinet, 8, 7.0);
        sec.set_chairs(6);
        assert_eq!(sec.chairs(), 6);
        sec.set_vibrato(5.0, 0.1);
        sec.note_on(261.63, 0.9);
        let (mut pl, mut pr) = (0.0f32, 0.0f32);
        let mut width = 0.0f32;
        for i in 0..fs as usize {
            let (l, r) = sec.process();
            assert!(l.is_finite() && r.is_finite());
            if i > fs as usize / 2 {
                pl = pl.max(l.abs());
                pr = pr.max(r.abs());
                width = width.max((l - r).abs());
            }
        }
        assert!(pl > 0.02 && pr > 0.02, "section silent: {pl} {pr}");
        // Humanized chairs decorrelate → a genuine stereo image (L != R).
        assert!(width > 0.01, "section not stereo/spread: width {width}");

        // set_width(0) collapses the pan toward centre (narrower L/R difference).
        sec.set_width(0.0);
        let mut narrow = 0.0f32;
        for _ in 0..fs as usize {
            let (l, r) = sec.process();
            narrow = narrow.max((l - r).abs());
        }
        assert!(narrow < width, "width=0 should narrow the image: {narrow} vs {width}");
        sec.note_off();
        for _ in 0..fs as usize {
            sec.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            let (l, r) = sec.process();
            tail = tail.max(l.abs()).max(r.abs());
        }
        assert!(tail < pl.max(pr), "section did not stop");
    }

    #[test]
    fn breath_mod_cv_normals_to_lfo() {
        let fs = 48_000.0;
        // Patched: a constant external CV drives the breath movement to a
        // steady offset (the onboard LFO is overridden).
        let mut v = Wind::new(fs, WindKind::Clarinet);
        v.note_on(261.63, 0.9);
        v.set_breath_mod(Some(1.0));
        v.set_breath_mod_depth(0.1);
        for _ in 0..fs as usize {
            v.process();
        }
        assert!(
            (v.breath_move - 1.1).abs() < 0.01,
            "patched CV should hold breath_move ≈ 1.1, got {}",
            v.breath_move
        );

        // Un-patched: the internal LFO moves the breath (not pinned to 1.0).
        v.set_breath_mod(None);
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for _ in 0..fs as usize * 3 {
            v.process();
            lo = lo.min(v.breath_move);
            hi = hi.max(v.breath_move);
        }
        assert!(hi - lo > 0.01, "normalled LFO should modulate breath, span {}", hi - lo);
    }

    #[test]
    fn flute_in_tune_with_even_harmonics() {
        // The flute is an open bore: in tune, all harmonics, strong fundamental.
        let fs = 48_000.0;
        for &f0 in &[261.63f32, 440.0, 523.25] {
            let mut v = Wind::new(fs, WindKind::Flute);
            v.note_on(f0, 0.9);
            let f = measured_hz(&mut v, fs, 24_000, 16_384);
            let cents = 1200.0 * (f / f0).log2();
            assert!(cents.abs() < 25.0, "flute {f0}Hz off by {cents:.1} cents ({f:.1})");
        }
        let f0 = 261.63f32;
        let mut v = Wind::new(fs, WindKind::Flute);
        v.note_on(f0, 0.9);
        for _ in 0..24_000 {
            v.process();
        }
        let buf: Vec<f32> = (0..16_384).map(|_| v.process()).collect();
        let (h2, h3) = (harmonic(&buf, f0, 2.0, fs), harmonic(&buf, f0, 3.0, fs));
        assert!(h2 > 0.3 * h3, "flute missing even harmonics: h2={h2:.4} h3={h3:.4}");
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
