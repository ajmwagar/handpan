//! # bowed-core
//!
//! Tier-2 physical modeling: the bowed string (violin / viola / cello / bass).
//! Unlike the struck modal instruments, a bowed string is a **self-sustained
//! oscillator** — a digital-waveguide string (two delay lines split at the bow
//! point, with a loss filter) driven by a **nonlinear stick-slip bow-friction**
//! interaction (McIntyre–Woodhouse–Schumacher / Smith–STK). It reuses the same
//! `no_std`, dependency-free discipline and drops into the same front-ends.
//!
//! Two front-doors:
//!
//! * [`Bowed`] — a single string. Bow it ([`Bowed::note_on`]) or pluck it
//!   ([`Bowed::pluck`]).
//! * [`BowedInstrument`] — a whole instrument: four strings tuned to the real
//!   open strings, sharing **one** resonant [`Body`]. Bow two at once for a
//!   **double stop**, or hop between them for **string crossing** — all coloured
//!   by the same body, exactly as a real instrument is.
//!
//! The body is a **modal bank** (a dozen resonators tuned to measured
//! violin/cello body resonances — the A0 air mode, the B1± main-wood modes, the
//! bridge hill) rather than a couple of formant filters. That reconstructs the
//! body's impulse response as a sum of modes: the same thing a convolution with
//! a measured body IR does, but as cheap IIR filters that stay real-time and
//! firmware-friendly.
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

use puget_dsp::mathf;
use puget_dsp::{Ensemble, Voice};

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

/// A single body mode: an impulse-normalized 2-pole resonator (one term of the
/// body's modal impulse response). `set` from (freq, Q, gain).
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
        // Pole radius from Q (bandwidth = f/Q).
        let r = mathf::exp(-core::f32::consts::PI * freq / (q * fs));
        Reso {
            a1: 2.0 * r * mathf::cos(w),
            a2: -(r * r),
            g: gain * mathf::sin(w), // impulse normalization
            y1: 0.0,
            y2: 0.0,
        }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.g * x + self.a1 * self.y1 + self.a2 * self.y2;
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
    let b = s.abs() + 0.75;
    let b2 = b * b;
    let mut o = 1.0 / (b2 * b2); // (|s| + 0.75)^-4
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
    /// The Persian **kamancheh**: a small upright spike fiddle whose spherical
    /// body is faced with a stretched skin membrane, giving a bright, nasal,
    /// vocal tone quite unlike the wooden violin. Bowed with a variable-tension
    /// horsehair bow; violin-ish register.
    Kamancheh,
    /// The Irish/traditional **fiddle**: physically the same instrument as the
    /// violin, GDAE tuning and all, but voiced for the trad repertoire —
    /// brighter, more open and raw than a refined solo violin, with a touch more
    /// bow-grip grit and a body that leans a little edgier. Built for driving
    /// reels and jigs over open-string drones and double stops, not for a
    /// covered concert-hall tone.
    Fiddle,
}

impl StringKind {
    /// The four open-string frequencies (Hz), low to high.
    pub fn open_strings(self) -> [f32; 4] {
        match self {
            // G3 D4 A4 E5
            StringKind::Violin => [196.00, 293.66, 440.00, 659.25],
            // C3 G3 D4 A4
            StringKind::Viola => [130.81, 196.00, 293.66, 440.00],
            // C2 G2 D3 A3
            StringKind::Cello => [65.41, 98.00, 146.83, 220.00],
            // E1 A1 D2 G2
            StringKind::Bass => [41.20, 55.00, 73.42, 98.00],
            // G3 D4 A4 D5 — a common Persian tuning (fifths + a fourth),
            // violin-ish range.
            StringKind::Kamancheh => [196.00, 293.66, 440.00, 587.33],
            // G3 D4 A4 E5 — identical to the violin; the fiddle is the same
            // instrument, tuned the same way.
            StringKind::Fiddle => [196.00, 293.66, 440.00, 659.25],
        }
    }
    /// String loss pole (higher = darker) and default bow position.
    fn string_def(self) -> (f32, f32) {
        match self {
            StringKind::Violin => (0.55, 0.13),
            StringKind::Viola => (0.58, 0.12),
            StringKind::Cello => (0.62, 0.11),
            StringKind::Bass => (0.66, 0.10),
            // Thin gut/steel strings on a small body; bowed near the bridge for
            // the bright, reedy kamancheh voice.
            StringKind::Kamancheh => (0.54, 0.14),
            // Same body as the violin, but a brighter (lower) loss pole and a
            // bow parked a touch closer to the bridge for the open, raw,
            // grittier folk-fiddle attack.
            StringKind::Fiddle => (0.51, 0.15),
        }
    }

    /// Bow-force → friction-`slope` range, `(slope_at_full_pressure, slope_at_zero_pressure)`.
    ///
    /// In the stick-slip friction table the saturated (stick / capture) region
    /// has half-width `0.25 / slope`, so a **smaller** slope means a **wider**
    /// stick region — a longer stick phase, a stronger Helmholtz corner, and a
    /// louder, richer tone. A real string gets louder and grippier with more bow
    /// force up to an over-pressure edge, so pressure must map to a *decreasing*
    /// slope (wider capture), not the increasing one that made firm bowing thin
    /// and quiet.
    ///
    /// The range is per-kind because the four instruments differ in string mass
    /// and admittance: the light violin/viola reach full Helmholtz motion at a
    /// low slope, while the heavy, low-tuned cello and bass would over-drive
    /// there, so their playable window sits at a higher (gentler) slope. The two
    /// endpoints bracket the playable dynamic range from soft (`p = 0`) to a
    /// controlled fortissimo at the over-pressure edge (`p = 1`); the midpoint
    /// stays near the historical `slope = 3` so existing calibration holds.
    fn bow_force_range(self) -> (f32, f32) {
        match self {
            StringKind::Violin => (1.5, 3.6),
            StringKind::Viola => (1.7, 2.8),
            StringKind::Cello => (1.25, 1.75),
            StringKind::Bass => (2.9, 4.6),
            // Light and small like the violin, so it grips at a low slope.
            StringKind::Kamancheh => (1.6, 3.4),
            // A violin body; grips hard and digs in for the driving trad
            // attack — a hair wider capture than the violin at full pressure.
            StringKind::Fiddle => (1.45, 3.5),
        }
    }
    /// Frequency scale applied to the violin body-mode template. (The bass uses
    /// its own table instead — see [`Body::new`].)
    fn body_scale(self) -> f32 {
        match self {
            StringKind::Violin => 1.0,
            // Measured (Iowa MIS): the viola body maps onto the violin template
            // almost perfectly under a single ~0.82 scale.
            StringKind::Viola => 0.82,
            StringKind::Cello => 0.52,
            StringKind::Bass => 0.20, // unused (bass has its own table)
            StringKind::Kamancheh => 1.0, // unused (kamancheh has its own table)
            // The fiddle reuses the violin body template, nudged ~5% up in
            // frequency so the wood cluster and bridge hill sit a little
            // brighter and edgier than a covered solo-violin tone.
            StringKind::Fiddle => 1.05,
        }
    }
    /// String-loop DC gain (< 1). Sets how long the string rings down once the
    /// bow lifts: closer to 1 = a longer, more singing tail. The kamancheh's
    /// light strings on a resonant skin body ring on noticeably longer than the
    /// heavily-damped orchestral strings.
    fn loop_gain(self) -> f32 {
        match self {
            StringKind::Kamancheh => 0.972,
            // Rings much like a violin, with a touch more singing tail to keep
            // open-string drones alive under the tune.
            StringKind::Fiddle => 0.955,
            _ => 0.95,
        }
    }
}

/// The resonant body: a modal bank tuned to measured violin body resonances
/// (scaled per instrument). Shared by all strings of a [`BowedInstrument`].
///
/// Template modes (violin, Hz): the A0 air resonance, the A1/B1± air-and-wood
/// cluster around 400–550, wood modes to ~1.1 kHz, and the broad bridge hill
/// around 1.7–3.5 kHz — the fixed formant peaks a long-term-average spectrum of
/// real violin recordings shows.
pub struct Body {
    modes: Vec<Reso>,
    dry: f32,
    wet: f32,
    makeup: f32,
}
impl Body {
    fn new(kind: StringKind, fs: f32) -> Self {
        // Violin body template (freq, Q, gain); scaled in frequency for the
        // viola and cello. Corroborated by a long-term-average spectrum of Iowa
        // MIS recordings: the A0 air mode, the A1/B1± cluster (~460-530, the
        // strongest peaks), wood modes, and the broad bridge hill.
        const TPL: &[(f32, f32, f32)] = &[
            (280.0, 14.0, 0.55), // A0 air
            (460.0, 22.0, 0.85), // B1-
            (530.0, 22.0, 1.00), // B1+ main wood (strongest)
            (700.0, 18.0, 0.50),
            (820.0, 16.0, 0.65), // wood (seen in LTAS)
            (1100.0, 12.0, 0.40),
            (1400.0, 9.0, 0.38),
            (1750.0, 7.0, 0.45), // lower bridge hill (seen in LTAS)
            (2500.0, 4.0, 0.55), // broad bridge hill
            (3500.0, 5.0, 0.28),
        ];
        // The double bass is NOT a scaled violin: its body energy is a tight
        // low cluster (~57-170 Hz) with a steep roll-off above ~200 Hz and no
        // meaningful bridge hill. Measured directly from Iowa MIS recordings.
        const TPL_BASS: &[(f32, f32, f32)] = &[
            (57.0, 18.0, 0.80),  // A0 main air
            (85.0, 20.0, 0.85),
            (101.0, 22.0, 1.00), // main wood (strongest)
            (127.0, 18.0, 0.85),
            (147.0, 14.0, 0.60),
            (170.0, 12.0, 0.50),
            (196.0, 10.0, 0.38),
            (270.0, 8.0, 0.20),
            (330.0, 6.0, 0.15),
            (500.0, 4.0, 0.12), // broad, weak; negligible above
        ];
        // The kamancheh is NOT a scaled violin either: a small spherical body
        // faced with a stretched skin membrane. That gives a strong low air/skin
        // resonance, a pronounced nasal mid formant (the kamancheh "honk"), and
        // bright skin radiation up top — vocal and reedy, without the violin's
        // B1± wood cluster.
        const TPL_KAMANCHEH: &[(f32, f32, f32)] = &[
            (330.0, 12.0, 0.70), // skin/air main resonance
            (600.0, 10.0, 1.00), // nasal formant (strongest)
            (950.0, 9.0, 0.55),
            (1500.0, 7.0, 0.45),
            (2200.0, 5.0, 0.55), // bright skin radiation
            (3200.0, 5.0, 0.35),
        ];
        let nyq = fs * 0.45;
        let mut modes = Vec::with_capacity(TPL.len());
        match kind {
            StringKind::Bass => {
                for &(f, q, g) in TPL_BASS {
                    if f < nyq {
                        modes.push(Reso::new(f, q, g, fs));
                    }
                }
            }
            StringKind::Kamancheh => {
                for &(f, q, g) in TPL_KAMANCHEH {
                    if f < nyq {
                        modes.push(Reso::new(f, q, g, fs));
                    }
                }
            }
            _ => {
                let scale = kind.body_scale();
                for &(f, q, g) in TPL {
                    let freq = f * scale;
                    if freq < nyq {
                        modes.push(Reso::new(freq, q, g, fs));
                    }
                }
            }
        }
        Body { modes, dry: 0.55, wet: 0.11, makeup: 3.4 }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let mut s = 0.0;
        for m in &mut self.modes {
            s += m.tick(x);
        }
        (x * self.dry + s * self.wet) * self.makeup
    }
}

/// A single bowed (or plucked) string — the waveguide + nonlinear bow, without
/// a body. `tick_raw` returns the bridge signal, which a [`Body`] then colours.
struct BowString {
    fs: f32,
    neck: Delay,
    bridge: Delay,
    string_filter: OnePole,
    bow_pos: f32,
    base_delay: f32,
    // Bow control (slewed so attacks/releases aren't clicks).
    max_vel_target: f32,
    vel_env: f32,
    /// Bow-string contact (1 = bow on the string, 0 = lifted off). Rises fast on
    /// attack, falls at `release_slew` on note-off; the friction coupling is
    /// scaled by it, so a lifted bow leaves the string to ring down *freely*
    /// (at just the loop loss) instead of staying in contact and damping it dry.
    contact: f32,
    /// Bow-force slew when releasing (target below the current level) — slower
    /// than the attack so a lifted bow tapers into the ring-down instead of
    /// cutting off dry.
    release_slew: f32,
    slope: f32,
    // Friction-slope endpoints (full-pressure, zero-pressure) for this kind.
    slope_lo: f32,
    slope_hi: f32,
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

impl BowString {
    fn new(fs: f32, kind: StringKind, seed: u32) -> Self {
        let (pole, bow_pos) = kind.string_def();
        let (slope_lo, slope_hi) = kind.bow_force_range();
        let max = (fs / 40.0) as usize + 4; // lowest ~40 Hz
        BowString {
            fs,
            neck: Delay::new(max),
            bridge: Delay::new(max),
            string_filter: OnePole::new(pole, kind.loop_gain()),
            bow_pos,
            base_delay: 100.0,
            max_vel_target: 0.0,
            vel_env: 0.0,
            contact: 0.0,
            release_slew: 0.0011, // ~20 ms: a graceful bow lift
            slope: 3.0,
            slope_lo,
            slope_hi,
            vib_phase: 0.0,
            vib_rate: 5.5,
            vib_depth: 0.0,
            rng: 0x1234_5678 ^ seed.wrapping_mul(0x9E37_79B9),
            noise_lp: 0.0,
            // The fiddle carries a touch more bow-grip noise baked in for its
            // raw, open trad grit; every other kind keeps its calibrated 0.12.
            noise_amt: match kind {
                StringKind::Fiddle => 0.16,
                _ => 0.12,
            },
            attack: 0,
            pluck_rem: 0,
            pluck_amp: 0.0,
        }
    }

    fn set_freq(&mut self, freq: f32) {
        // Loop length accounts for the loss filter + last_out() cache phase
        // delay (~3.2 samples) so the pitch doesn't run flat.
        self.base_delay = (self.fs / freq - 3.2).max(4.0);
        self.retune(0.0);
    }

    #[inline]
    fn white(&mut self) -> f32 {
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

    fn note_on(&mut self, freq: f32, bow_velocity: f32, bow_pressure: f32) {
        self.set_freq(freq);
        self.set_bow(bow_velocity, bow_pressure);
        self.contact = 1.0; // the bow is already on the string
        self.attack = (0.03 * self.fs) as u32; // ~30 ms onset scratch
    }

    fn note_off(&mut self) {
        self.max_vel_target = 0.0;
    }

    fn pluck(&mut self, freq: f32, velocity: f32) {
        self.set_freq(freq);
        self.max_vel_target = 0.0;
        self.vel_env = 0.0;
        self.pluck_rem = (0.004 * self.fs) as u32;
        self.pluck_amp = 0.6 * velocity.clamp(0.0, 1.0);
    }

    fn set_bow(&mut self, bow_velocity: f32, bow_pressure: f32) {
        self.max_vel_target = 0.03 + 0.22 * bow_velocity.clamp(0.0, 1.0);
        // More bow force WIDENS the stick region (louder, richer), so pressure
        // interpolates the friction slope from its zero-pressure (soft, high
        // slope) endpoint down to its full-pressure (loud, low slope) endpoint.
        let p = bow_pressure.clamp(0.0, 1.0);
        self.slope = self.slope_hi + (self.slope_lo - self.slope_hi) * p;
    }

    fn set_bow_position(&mut self, pos: f32) {
        self.bow_pos = (0.06 + 0.14 * pos.clamp(0.0, 1.0)).clamp(0.02, 0.5);
        self.retune(0.0);
    }

    fn set_brightness(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        self.string_filter.pole = 0.72 - 0.30 * a;
        self.noise_amt = 0.06 + 0.14 * a;
    }

    fn set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        self.vib_rate = rate_hz.max(0.0);
        self.vib_depth = depth.clamp(0.0, 0.05);
    }

    /// Is this string still producing meaningful energy?
    #[inline]
    fn active(&self) -> bool {
        self.max_vel_target > 0.0 || self.vel_env > 1e-4 || self.pluck_rem > 0
    }

    /// One sample of the raw bridge signal (pre-body).
    #[inline]
    fn tick_raw(&mut self) -> f32 {
        // Faster to reach a louder bow (crisp attack), slower to fall away (a
        // graceful release into the ring-down, so notes don't cut off dry).
        let slew = if self.max_vel_target < self.vel_env { self.release_slew } else { 0.0025 };
        self.vel_env += slew * (self.max_vel_target - self.vel_env);
        // Bow-string contact: fast to land on the string, slow to lift off. When
        // the bow lifts (target 0) this fades to 0 and the string rings freely.
        let contact_target = if self.max_vel_target > 0.0 { 1.0 } else { 0.0 };
        let contact_rate = if contact_target > self.contact { 0.01 } else { self.release_slew };
        self.contact += contact_rate * (contact_target - self.contact);

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
            return self.bridge.last_out();
        }

        // Bow-friction noise + onset-scratch burst at the attack.
        let n = self.white();
        self.noise_lp += 0.25 * (n - self.noise_lp);
        let mut scratch = 0.0;
        if self.attack > 0 {
            self.attack -= 1;
            scratch = self.noise_lp * 0.5 * (self.attack as f32 / (0.03 * self.fs));
        }
        let noise = self.noise_lp * self.noise_amt * self.vel_env + scratch;

        let bridge_refl = -self.string_filter.tick(self.bridge.last_out());
        let nut_refl = -self.neck.last_out();
        let string_vel = bridge_refl + nut_refl;
        let delta = (self.vel_env + noise) - string_vel;
        // Scale the friction interaction by bow contact: at full contact this is
        // the normal bowed drive; as the bow lifts, it fades to 0 and the loop
        // reflects freely, so the string rings down (not damped by a resting bow).
        let new_vel = delta * bow_friction(delta, self.slope) * self.contact;
        self.neck.tick(bridge_refl + new_vel);
        self.bridge.tick(nut_refl + new_vel);
        self.bridge.last_out()
    }
}

/// A single bowed string with its own body (one voice).
pub struct Bowed {
    string: BowString,
    body: Body,
}

impl Bowed {
    pub fn new(fs: f32, kind: StringKind) -> Self {
        Bowed { string: BowString::new(fs, kind, kind as u32), body: Body::new(kind, fs) }
    }

    /// Start a note: `freq` Hz, `bow_velocity` and `bow_pressure` in [0, 1].
    pub fn note_on(&mut self, freq: f32, bow_velocity: f32, bow_pressure: f32) {
        self.string.note_on(freq, bow_velocity, bow_pressure);
    }

    /// Lift the bow — the string rings down.
    pub fn note_off(&mut self) {
        self.string.note_off();
    }

    /// Pizzicato: pluck the string. `velocity` in [0, 1].
    pub fn pluck(&mut self, freq: f32, velocity: f32) {
        self.string.pluck(freq, velocity);
    }

    /// Continuous bow control (during a note): speed and pressure in [0, 1].
    pub fn set_bow(&mut self, bow_velocity: f32, bow_pressure: f32) {
        self.string.set_bow(bow_velocity, bow_pressure);
    }

    /// Bow position: 0 = sul tasto (mellow), 1 = sul ponticello (glassy).
    pub fn set_bow_position(&mut self, pos: f32) {
        self.string.set_bow_position(pos);
    }

    /// Timbral brightness: 0 = dark/damped, 1 = bright/singing.
    pub fn set_brightness(&mut self, amount: f32) {
        self.string.set_brightness(amount);
    }

    /// Vibrato: rate (Hz) and depth (fraction of a semitone-ish).
    pub fn set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        self.string.set_vibrato(rate_hz, depth);
    }

    /// Decorrelate this player from other copies of the same instrument, so a
    /// stacked section shimmers like many players rather than one loud unison:
    /// re-seeds the bow-friction noise and desyncs the vibrato phase.
    pub fn humanize(&mut self, seed: u32) {
        self.string.rng ^= seed.wrapping_mul(0x9E37_79B9) | 1;
        let f = ((seed & 0xffff) as f32) / 65_535.0;
        self.string.vib_phase = f * core::f32::consts::TAU;
    }

    /// One mono sample.
    #[inline]
    pub fn process(&mut self) -> f32 {
        let raw = self.string.tick_raw();
        self.body.tick(raw)
    }
}

/// A whole bowed instrument: four strings tuned to the real open strings,
/// sharing one resonant [`Body`]. Play notes with [`BowedInstrument::note_on`]
/// (up to four at once → double/triple stops); the shared body means every note
/// is coloured by the same instrument, and released strings ring down naturally.
pub struct BowedInstrument {
    strings: [BowString; 4],
    body: Body,
    open: [f32; 4],
    playing: [Option<f32>; 4], // freq currently bowed on each string (None = ringing/idle)
}

impl BowedInstrument {
    pub fn new(fs: f32, kind: StringKind) -> Self {
        BowedInstrument {
            strings: [
                BowString::new(fs, kind, kind as u32 * 4),
                BowString::new(fs, kind, kind as u32 * 4 + 1),
                BowString::new(fs, kind, kind as u32 * 4 + 2),
                BowString::new(fs, kind, kind as u32 * 4 + 3),
            ],
            body: Body::new(kind, fs),
            open: kind.open_strings(),
            playing: [None; 4],
        }
    }

    /// Choose the string to play a frequency on: the highest open string not
    /// above the note (so it can be reached by stopping up the fingerboard).
    fn pick(&self, freq: f32) -> usize {
        let mut idx = 0;
        for i in 0..4 {
            if self.open[i] <= freq * 1.0001 {
                idx = i;
            }
        }
        idx
    }

    /// Bow a note. Call twice on notes that map to different strings for a
    /// double stop. Returns the string index used.
    pub fn note_on(&mut self, freq: f32, bow_velocity: f32, bow_pressure: f32) -> usize {
        let i = self.pick(freq);
        self.strings[i].note_on(freq, bow_velocity, bow_pressure);
        self.playing[i] = Some(freq);
        i
    }

    /// Pluck a note (pizzicato). Returns the string index used.
    pub fn pluck(&mut self, freq: f32, velocity: f32) -> usize {
        let i = self.pick(freq);
        self.strings[i].pluck(freq, velocity);
        self.playing[i] = None;
        i
    }

    /// Lift the bow on whichever string is playing `freq` (nearest match).
    pub fn note_off(&mut self, freq: f32) {
        let mut best = None;
        let mut bestd = f32::MAX;
        for i in 0..4 {
            if let Some(f) = self.playing[i] {
                let d = (f - freq).abs();
                if d < bestd {
                    bestd = d;
                    best = Some(i);
                }
            }
        }
        if let Some(i) = best {
            self.strings[i].note_off();
            self.playing[i] = None;
        }
    }

    /// Lift every bow (all strings ring down).
    pub fn note_off_all(&mut self) {
        for i in 0..4 {
            self.strings[i].note_off();
            self.playing[i] = None;
        }
    }

    /// Bow position for all strings: 0 = sul tasto, 1 = sul ponticello.
    pub fn set_bow_position(&mut self, pos: f32) {
        for s in &mut self.strings {
            s.set_bow_position(pos);
        }
    }

    /// Brightness for all strings.
    pub fn set_brightness(&mut self, amount: f32) {
        for s in &mut self.strings {
            s.set_brightness(amount);
        }
    }

    /// Vibrato for all strings.
    pub fn set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        for s in &mut self.strings {
            s.set_vibrato(rate_hz, depth);
        }
    }

    /// How many strings are currently sounding (bowed or ringing).
    pub fn voices(&self) -> usize {
        self.strings.iter().filter(|s| s.active()).count()
    }

    /// One mono sample: all four strings summed through the shared body.
    #[inline]
    pub fn process(&mut self) -> f32 {
        let mut raw = 0.0;
        for s in &mut self.strings {
            raw += s.tick_raw();
        }
        self.body.tick(raw)
    }
}

/// How a section note is played on each string: bowed (arco) or plucked (pizz).
#[derive(Clone, Copy)]
pub enum BowEvent {
    Arco { velocity: f32, pressure: f32 },
    Pizz { velocity: f32 },
}

impl Voice for Bowed {
    type Event = BowEvent;
    fn humanize(&mut self, seed: u32) {
        Bowed::humanize(self, seed);
    }
    fn trigger(&mut self, freq: f32, ev: BowEvent) {
        match ev {
            BowEvent::Arco { velocity, pressure } => self.note_on(freq, velocity, pressure),
            BowEvent::Pizz { velocity } => self.pluck(freq, velocity),
        }
    }
    fn release(&mut self) {
        self.note_off();
    }
    fn process(&mut self) -> f32 {
        Bowed::process(self)
    }
}

/// A **string section**: humanized [`Bowed`] players (each its own string +
/// body) stacked across the stereo field (the shared [`Ensemble`]). The desk
/// count is the "how many players" control; `set_spread`/`set_width` are the
/// tuning and stereo-image macros. Supports arco and pizzicato.
pub struct BowedEnsemble {
    section: Ensemble<Bowed>,
}

impl BowedEnsemble {
    /// A section of up to `max_desks` players, `spread_cents` intonation spread.
    pub fn new(fs: f32, kind: StringKind, max_desks: usize, spread_cents: f32) -> Self {
        // Bows don't land together — up to ~40 ms of onset spread.
        let section = Ensemble::new(fs, max_desks, spread_cents, 40.0, |_| Bowed::new(fs, kind));
        BowedEnsemble { section }
    }

    /// How many players are sounding (the "desks" encoder), clamped `[1, max]`.
    pub fn set_desks(&mut self, n: usize) {
        self.section.set_active(n);
    }
    /// Number of active desks.
    pub fn desks(&self) -> usize {
        self.section.active()
    }
    /// Intonation spread across the section, in cents (intimate ↔ wide).
    pub fn set_spread(&mut self, cents: f32) {
        self.section.set_spread(cents);
    }
    /// Stereo width, 0..1 (mono ↔ full field).
    pub fn set_width(&mut self, width: f32) {
        self.section.set_width(width);
    }
    /// Bow a note across the section.
    pub fn note_on(&mut self, freq: f32, bow_velocity: f32, bow_pressure: f32) {
        self.section.trigger(freq, BowEvent::Arco { velocity: bow_velocity, pressure: bow_pressure });
    }
    /// Pizzicato across the section.
    pub fn pluck(&mut self, freq: f32, velocity: f32) {
        self.section.trigger(freq, BowEvent::Pizz { velocity });
    }
    /// Release the section (bows lift; strings ring down).
    pub fn note_off(&mut self) {
        self.section.release();
    }
    /// Vibrato — depth shared, rate spread per desk.
    pub fn set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        self.section.for_each_voice(|v, s| v.set_vibrato(rate_hz * s, depth));
    }
    /// Bow position, section-wide (0 = sul tasto, 1 = sul ponticello).
    pub fn set_bow_position(&mut self, pos: f32) {
        self.section.for_each_voice(|v, _| v.set_bow_position(pos));
    }
    /// Brightness, section-wide.
    pub fn set_brightness(&mut self, amount: f32) {
        self.section.for_each_voice(|v, _| v.set_brightness(amount));
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
    fn bow_sustains_then_stops_and_stays_bounded() {
        for kind in [StringKind::Violin, StringKind::Cello, StringKind::Bass] {
            let mut v = Bowed::new(48_000.0, kind);
            let freq = match kind {
                StringKind::Bass => 55.0,
                StringKind::Cello => 130.0,
                _ => 440.0,
            };
            v.note_on(freq, 0.7, 0.5);
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
        let mut v = Bowed::new(fs, StringKind::Violin);
        v.note_on(440.0, 0.7, 0.5);
        let f = measured_hz(&mut v, fs, 24_000, 8_192);
        let cents = 1200.0 * (f / 440.0).log2();
        assert!(cents.abs() < 15.0, "violin A4 off by {cents:.1} cents ({f:.1} Hz)");

        let mut c = Bowed::new(fs, StringKind::Cello);
        c.note_on(130.81, 0.7, 0.5);
        let f = measured_hz(&mut c, fs, 24_000, 16_384);
        let cents = 1200.0 * (f / 130.81).log2();
        assert!(cents.abs() < 15.0, "cello C3 off by {cents:.1} cents ({f:.1} Hz)");
    }

    #[test]
    fn kamancheh_sounds_and_is_in_tune() {
        let fs = 48_000.0;
        let mut k = Bowed::new(fs, StringKind::Kamancheh);
        k.note_on(392.0, 0.7, 0.5); // G4
        // It self-oscillates to an audible level.
        let mut peak = 0.0f32;
        for _ in 0..24_000 {
            peak = peak.max(k.process().abs());
        }
        assert!(peak > 0.02, "kamancheh too quiet: {peak}");
        let f = measured_hz(&mut k, fs, 8_000, 8_192);
        let cents = 1200.0 * (f / 392.0).log2();
        assert!(cents.abs() < 15.0, "kamancheh G4 off by {cents:.1} cents ({f:.1} Hz)");
    }

    #[test]
    fn fiddle_sounds_and_is_in_tune() {
        let fs = 48_000.0;
        let mut f = Bowed::new(fs, StringKind::Fiddle);
        f.note_on(440.0, 0.7, 0.5); // A4 (open A)
        // It self-oscillates to an audible level.
        let mut peak = 0.0f32;
        for _ in 0..24_000 {
            peak = peak.max(f.process().abs());
        }
        assert!(peak > 0.02, "fiddle too quiet: {peak}");
        let hz = measured_hz(&mut f, fs, 8_000, 8_192);
        let cents = 1200.0 * (hz / 440.0).log2();
        assert!(cents.abs() < 15.0, "fiddle A4 off by {cents:.1} cents ({hz:.1} Hz)");
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
        for _ in 0..fs as usize * 2 {
            v.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            tail = tail.max(v.process().abs());
        }
        assert!(tail < peak * 0.5, "pluck did not decay: tail {tail} vs peak {peak}");
    }

    #[test]
    fn ensemble_stacks_desks_and_is_stereo() {
        let fs = 48_000.0;
        let mut sec = BowedEnsemble::new(fs, StringKind::Violin, 8, 6.0);
        sec.set_desks(6);
        assert_eq!(sec.desks(), 6);
        sec.set_vibrato(5.5, 0.02);
        sec.note_on(440.0, 0.7, 0.5);
        let (mut pl, mut pr, mut width) = (0.0f32, 0.0f32, 0.0f32);
        for i in 0..48_000 {
            let (l, r) = sec.process();
            assert!(l.is_finite() && r.is_finite());
            if i > 24_000 {
                pl = pl.max(l.abs());
                pr = pr.max(r.abs());
                width = width.max((l - r).abs());
            }
        }
        assert!(pl > 0.02 && pr > 0.02, "section silent: {pl} {pr}");
        assert!(width > 0.01, "section not spread/stereo: {width}");
        sec.note_off();
        for _ in 0..48_000 * 2 {
            sec.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            let (l, r) = sec.process();
            tail = tail.max(l.abs()).max(r.abs());
        }
        assert!(tail < pl.max(pr), "section did not decay");
    }

    #[test]
    fn double_stop_plays_two_strings() {
        let fs = 48_000.0;
        let mut v = BowedInstrument::new(fs, StringKind::Violin);
        // Open D (293.66) and open A (440) — a perfect fifth double stop.
        let d = v.note_on(293.66, 0.7, 0.5);
        let a = v.note_on(440.0, 0.7, 0.5);
        assert_ne!(d, a, "double stop landed on the same string");
        let mut peak = 0.0f32;
        for i in 0..48_000 {
            let s = v.process();
            assert!(s.is_finite());
            if i > 24_000 {
                peak = peak.max(s.abs());
            }
        }
        assert!(peak > 0.02, "double stop silent: {peak}");
        assert!(peak < 60.0, "double stop runaway: {peak}");
        assert!(v.voices() >= 2, "expected two sounding strings, got {}", v.voices());

        // Release one; the other keeps singing.
        v.note_off(440.0);
        let mut still = 0.0f32;
        for _ in 0..24_000 {
            still = still.max(v.process().abs());
        }
        assert!(still > 0.02, "remaining string stopped too");
    }

    #[test]
    fn string_crossing_picks_expected_strings() {
        let v = BowedInstrument::new(48_000.0, StringKind::Cello);
        // Cello open strings C2 G2 D3 A3 → indices 0..3.
        assert_eq!(v.pick(65.41), 0);
        assert_eq!(v.pick(98.0), 1);
        assert_eq!(v.pick(150.0), 2); // just above D3
        assert_eq!(v.pick(300.0), 3); // above A3, top string
    }

    /// Settle a freshly-bowed voice, then measure (rms, peak, all-finite) over a
    /// window — the loudness a realism audit reads off the pressure×velocity grid.
    fn bowed_rms(kind: StringKind, freq: f32, bv: f32, pressure: f32) -> (f32, f32, bool) {
        let fs = 48_000.0;
        let mut v = Bowed::new(fs, kind);
        v.note_on(freq, bv, pressure);
        let mut finite = true;
        for _ in 0..24_000 {
            if !v.process().is_finite() {
                finite = false;
            }
        }
        let (mut acc, mut peak) = (0.0f64, 0.0f32);
        let window = 16_384;
        for _ in 0..window {
            let s = v.process();
            if !s.is_finite() {
                finite = false;
            }
            peak = peak.max(s.abs());
            acc += (s as f64) * (s as f64);
        }
        ((acc / window as f64).sqrt() as f32, peak, finite)
    }

    /// The bow-force cliff regression: a real string gets *louder* (never quieter)
    /// with more bow force up to an over-pressure edge. Before the pressure→slope
    /// mapping fix the stick region collapsed as pressure rose, so firm bowing got
    /// dramatically quieter (violin bv0.7: p0.0 rms ≈ 24 → p0.5 ≈ 5). Assert
    /// loudness is monotonic-non-decreasing vs pressure (within tolerance) and
    /// stays audible at full pressure, for violin and cello.
    #[test]
    fn loudness_is_monotonic_in_pressure() {
        // 15% local slack absorbs the friction model's regime-transition ripple
        // while still catching any real collapse (the bug was a ~4x drop).
        const TOL: f32 = 0.85;
        for (kind, freq) in [(StringKind::Violin, 440.0), (StringKind::Cello, 130.81)] {
            for &bv in &[0.7f32] {
                let pressures = [0.0f32, 0.25, 0.5, 0.75, 1.0];
                let mut rms = [0.0f32; 5];
                for (i, &p) in pressures.iter().enumerate() {
                    let (r, pk, fin) = bowed_rms(kind, freq, bv, p);
                    assert!(fin, "{kind:?} non-finite at pressure {p}");
                    assert!(pk < 80.0, "{kind:?} runaway ({pk}) at pressure {p}");
                    rms[i] = r;
                }
                // Monotonic-non-decreasing vs a running maximum (within TOL).
                let mut run = rms[0];
                for (i, &r) in rms.iter().enumerate() {
                    assert!(
                        r >= run * TOL,
                        "{kind:?} bv{bv} loudness cliff at p={}: rms {r:.2} < {:.2} (running max {run:.2}); full curve {rms:?}",
                        pressures[i],
                        run * TOL,
                    );
                    run = run.max(r);
                }
                // Firm bowing must be clearly louder than light bowing — the whole
                // point of the fix — and fortissimo must be audible, not a dropout.
                assert!(
                    rms[4] > rms[1] * 1.2,
                    "{kind:?} firm bowing not louder than light: p1 {:.2} vs p0.25 {:.2}",
                    rms[4],
                    rms[1],
                );
                assert!(rms[4] > 2.0, "{kind:?} fortissimo too quiet: {:.2}", rms[4]);
            }
        }
    }

    /// The full playability sweep: every StringKind, over the pressure×bow-velocity
    /// grid, must self-oscillate cleanly — finite, no blow-up, no silent dropout —
    /// and loudness must be monotonic-non-decreasing vs pressure (within tolerance)
    /// on the coarse playable grid.
    #[test]
    fn bow_force_sweep_stable_and_monotonic_all_kinds() {
        const TOL: f32 = 0.80;
        let pressures = [0.0f32, 0.25, 0.5, 0.75, 1.0];
        let bowvels = [0.4f32, 0.7, 1.0];
        for (kind, freq) in [
            (StringKind::Violin, 440.0),
            (StringKind::Viola, 293.66),
            (StringKind::Cello, 130.81),
            (StringKind::Bass, 55.0),
        ] {
            for &bv in &bowvels {
                let mut run = -1.0f32;
                for &p in &pressures {
                    let (r, pk, fin) = bowed_rms(kind, freq, bv, p);
                    assert!(fin, "{kind:?} bv{bv} p{p}: non-finite (NaN/denormal blow-up)");
                    assert!(pk < 80.0, "{kind:?} bv{bv} p{p}: runaway peak {pk}");
                    assert!(r > 0.5, "{kind:?} bv{bv} p{p}: silent dropout, rms {r}");
                    if run >= 0.0 {
                        assert!(
                            r >= run * TOL,
                            "{kind:?} bv{bv}: loudness collapse at p={p}: rms {r:.2} < {:.2} (running max {run:.2})",
                            run * TOL,
                        );
                    }
                    run = run.max(r);
                }
            }
        }
    }
}
