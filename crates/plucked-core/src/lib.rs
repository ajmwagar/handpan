//! # plucked-core
//!
//! Plucked-string physical modeling for lutes, zithers and electric strings:
//! - the **[`Cifteli`]** — the two-string Albanian long-neck lute (a fixed
//!   **drone** string plus a fretted **melody** string),
//! - the **[`Gayageum`]** — the Korean sanjo zither: a pool of long silk strings
//!   with a warm paulownia body and deep left-hand *nonghyeon* (vibrato + bends),
//! - the **[`Basitar`]** — the two-string bass/guitar hybrid: heavy strings
//!   tuned a fifth apart through a pickup and amp grind (power-chord basslines),
//! - the **[`Santur`]** — the Persian trapezoidal hammered dulcimer: courses of
//!   near-unison steel strings *struck* by a mallet into a shimmering chorus,
//! - the **[`Tar`]** — the Persian long-neck lute: doubled courses plucked with
//!   a brass plectrum over a bright, nasal *skin-membrane* top.
//!
//! Each string is an extended **Karplus-Strong** waveguide — a fractional delay
//! line closed by a loss/damping filter, excited by a noise burst — upgraded
//! past the textbook model with a **stiffness-dispersion allpass** (partials
//! stretched sharp, pitch-compensated), a **pluck-position comb**, velocity-
//! dependent brightness, gentle loop saturation, and per-sample **glide +
//! vibrato** for expressive left-hand technique. A few resonators colour the
//! wooden body. Reuses the shared [`puget_dsp`] foundation (fractional delay +
//! math); `no_std`, dependency-free.
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

extern crate alloc;
use alloc::vec::Vec;

use puget_dsp::mathf;
use puget_dsp::Delay;

/// Two-argument arctangent built on the shared `mathf::atan` (which only covers
/// the principal branch). Kept local so the crate stays `no_std` + `libm`.
#[inline]
fn atan2f(y: f32, x: f32) -> f32 {
    use core::f32::consts::{FRAC_PI_2, PI};
    if x > 0.0 {
        mathf::atan(y / x)
    } else if x < 0.0 {
        if y >= 0.0 {
            mathf::atan(y / x) + PI
        } else {
            mathf::atan(y / x) - PI
        }
    } else if y > 0.0 {
        FRAC_PI_2
    } else if y < 0.0 {
        -FRAC_PI_2
    } else {
        0.0
    }
}

/// Maximum second-order dispersion-allpass sections cascaded in a string loop.
const DISP_MAX_STAGES: usize = 4;

/// One second-order allpass section — the building block of the string-stiffness
/// dispersion filter. A cascade of these has a tunable group-delay bump (set by
/// pole radius `r` and centre `fpeak`); partials **above** the bump are advanced
/// (stretched **sharp**), which is exactly the inharmonicity of a real stiff
/// steel/bass string. First-order allpasses can't do this in the bass — their
/// phase is flat at low frequencies, so a heavy low string comes out harmonic.
#[derive(Clone, Copy, Default)]
struct BiquadAP {
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}
impl BiquadAP {
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        // Allpass: H(z) = (a2 + a1 z⁻¹ + z⁻²) / (1 + a1 z⁻¹ + a2 z⁻²).
        let y = self.a2 * x + self.a1 * self.x1 + self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

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
///
/// On top of the classic delay-line-plus-loss-filter it adds the upgrades that
/// separate a toy KS from a real physical string:
/// - **fractional delay** (the shared [`Delay`] interpolates) for exact tuning;
/// - a **stiffness allpass** in the loop, stretching partials sharp like a real
///   string, with its phase delay compensated so the pitch stays true;
/// - a **pluck-position comb** on the excitation, notching the harmonics the way
///   plucking a string a fraction of its length along does;
/// - **velocity-dependent brightness** (a harder pluck opens the loop filter);
/// - gentle **loop saturation** for harmonic warmth;
/// - per-sample **glide + vibrato** — the substrate for the gayageum's
///   *nonghyeon* left hand.
struct PluckString {
    fs: f32,
    delay: Delay,
    lp_y1: f32,
    /// Loop damping (brightness): higher = darker, faster HF decay.
    damp: f32,
    /// Loop gain (< 1): overall sustain.
    decay: f32,
    /// String-stiffness dispersion: a cascade of `disp_stages` identical
    /// second-order allpass sections (shared coefficients `disp_a1`/`disp_a2`).
    /// `disp_stages == 0` disables dispersion entirely.
    disp_a1: f32,
    disp_a2: f32,
    disp_stages: usize,
    disp: [BiquadAP; DISP_MAX_STAGES],
    /// Loop saturation drive (0 = clean).
    sat: f32,
    /// Noise-burst excitation state.
    rng: u32,
    exc_rem: u32,
    /// Length of the current excitation burst (for the attack fade-in).
    exc_len: u32,
    exc_amp: f32,
    /// Per-note humanization (0 = mechanical, 1 = loose): subtle velocity and
    /// onset-timing jitter so repeated notes aren't identical. Pitch-neutral —
    /// it never detunes the string.
    humanize: f32,
    /// Pluck-position comb: a short delay whose output is subtracted from the
    /// excitation, plus its depth (0 = off).
    comb: Delay,
    comb_amt: f32,
    /// External excitation injected into the loop this sample (sympathetic
    /// coupling from a neighbouring string via the shared bridge). Consumed and
    /// cleared each `tick`.
    ext_in: f32,
    /// Samples still to wait before the pending excitation burst begins — a
    /// per-string strike micro-timing offset (a mallet doesn't contact every
    /// string of a course on the same sample).
    onset: u32,
    /// Transient extra brightness from the last pluck's velocity (decays away).
    vel_bright: f32,
    /// Nominal (open) pitch of the string in Hz.
    base_hz: f32,
    /// Static bend target, in cents, that `bend_cur` glides toward.
    bend_cents: f32,
    /// Currently-sounding bend offset in cents (glides toward `bend_cents`).
    bend_cur: f32,
    /// Glide coefficient per sample (how fast `bend_cur` chases `bend_cents`).
    glide: f32,
    /// Vibrato rate (Hz) and depth (cents) — the *nonghyeon* wobble.
    vib_rate: f32,
    vib_depth: f32,
    vib_phase: f32,
    /// Whether this string is the one the left hand is working (vibrato on).
    vib_on: bool,
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
            disp_a1: 0.0,
            disp_a2: 0.0,
            disp_stages: 0,
            disp: [BiquadAP::default(); DISP_MAX_STAGES],
            sat: 0.0,
            rng: 0x9E37_79B9 ^ seed,
            exc_rem: 0,
            exc_len: 1,
            exc_amp: 0.0,
            humanize: 0.25,
            comb: Delay::new(max),
            comb_amt: 0.0,
            ext_in: 0.0,
            onset: 0,
            vel_bright: 0.0,
            base_hz: 196.0,
            bend_cents: 0.0,
            bend_cur: 0.0,
            glide: 1.0,
            vib_rate: 5.5,
            vib_depth: 0.0,
            vib_phase: 0.0,
            vib_on: false,
        }
    }
    #[inline]
    fn white(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
    /// Configure the string-stiffness dispersion: `stages` second-order allpass
    /// sections with a group-delay bump of radius `r` (0..1) centred at
    /// `fpeak_hz`. Partials above `fpeak_hz` are stretched sharp. `stages == 0`
    /// (or `r == 0`) turns dispersion off.
    fn set_dispersion(&mut self, r: f32, fpeak_hz: f32, stages: usize) {
        let stages = stages.min(DISP_MAX_STAGES);
        if stages == 0 || r <= 0.0 {
            self.disp_stages = 0;
            self.disp_a1 = 0.0;
            self.disp_a2 = 0.0;
            return;
        }
        self.disp_stages = stages;
        let theta = core::f32::consts::TAU * fpeak_hz / self.fs;
        self.disp_a1 = -2.0 * r * mathf::cos(theta);
        self.disp_a2 = r * r;
        for s in &mut self.disp {
            s.a1 = self.disp_a1;
            s.a2 = self.disp_a2;
        }
    }

    /// Phase delay (samples) of the whole dispersion cascade at radian frequency
    /// `w`. For one section H = e^{-2jw}·conj(D)/D, so its phase delay is
    /// `2 + 2·∠D(e^{jw})/w`; the cascade multiplies by the stage count. Evaluated
    /// only at the (sub-`fpeak`) fundamental, where ∠D stays on the principal
    /// branch, so no unwrapping is needed.
    #[inline]
    fn disp_phase_delay(&self, w: f32) -> f32 {
        if self.disp_stages == 0 {
            return 0.0;
        }
        let (a1, a2) = (self.disp_a1, self.disp_a2);
        let re = 1.0 + a1 * mathf::cos(w) + a2 * mathf::cos(2.0 * w);
        let im = -(a1 * mathf::sin(w) + a2 * mathf::sin(2.0 * w));
        let pd_one = 2.0 + 2.0 * atan2f(im, re) / w;
        pd_one * self.disp_stages as f32
    }

    fn set_freq(&mut self, freq: f32) {
        let raw = self.fs / freq;
        let w = core::f32::consts::TAU * freq / self.fs;
        // The loop's fractional-delay length must make the *total* loop delay
        // equal one period. Besides the delay line the loop carries: the read
        // cache (1 sample), the one-pole loss filter, and the dispersion
        // allpass. Subtract each one's phase delay *at the fundamental* so the
        // open string lands on pitch instead of a few cents flat.
        let a = self.damp.clamp(0.0, 0.999);
        let lp_pd = if w > 1e-6 {
            mathf::atan(a * mathf::sin(w) / (1.0 - a * mathf::cos(w))) / w
        } else {
            a / (1.0 - a)
        };
        let ap_pd = self.disp_phase_delay(w);
        self.delay.set_delay((raw - 1.0 - lp_pd - ap_pd).max(2.0));
    }

    /// Inject external excitation into the loop next `tick` (sympathetic bridge
    /// coupling from a neighbouring string).
    #[inline]
    fn inject(&mut self, x: f32) {
        self.ext_in += x;
    }

    /// Add to the delay before the pending excitation burst begins (a per-string
    /// strike micro-timing offset). Additive so it composes with any humanization
    /// jitter already applied at pluck/strike time.
    #[inline]
    fn set_onset(&mut self, samples: u32) {
        self.onset = self.onset.saturating_add(samples);
    }
    /// Per-note humanization amount (0 = mechanical, 1 = loose).
    #[inline]
    fn set_humanize(&mut self, amount: f32) {
        self.humanize = amount.clamp(0.0, 1.0);
    }
    /// Apply humanization to a note's velocity + onset. Velocity gets a small
    /// random trim; the onset a small random delay. Never touches pitch, so the
    /// string stays exactly in tune.
    #[inline]
    fn humanize_note(&mut self, velocity: f32) -> f32 {
        self.onset = 0;
        let mut v = velocity.clamp(0.0, 1.0);
        if self.humanize > 0.0 {
            v = (v * (1.0 + 0.10 * self.humanize * self.white())).clamp(0.0, 1.0);
            let u = self.white() * 0.5 + 0.5; // 0..1
            self.onset = (self.humanize * 0.004 * self.fs * u) as u32;
        }
        v
    }
    /// Pluck: excite the loop with a comb-shaped noise burst ~one period long.
    fn pluck(&mut self, freq: f32, velocity: f32) {
        self.base_hz = freq.max(1.0);
        self.set_freq(self.base_hz);
        let period = self.fs / self.base_hz;
        self.exc_rem = period as u32;
        self.exc_len = self.exc_rem.max(1);
        let v = self.humanize_note(velocity);
        self.exc_amp = v;
        // Pluck a fraction β along the string → comb notch every 1/β harmonics.
        // β ≈ 0.13 (near the bridge) gives the bright lute tone; the comb depth
        // is kept moderate so the pluck reads warm rather than hollow/nasal.
        self.comb.set_delay((0.13 * period).max(1.0));
        self.comb_amt = 0.75;
        self.vel_bright = v;
    }
    /// Strike: a hard, light mallet (santur mezrab) hitting the string. Unlike a
    /// pluck it's a sharp, short contact near the bridge — a brief brilliant
    /// transient and a bright loop, rather than the slower shaped release of a
    /// fingered pluck.
    fn strike(&mut self, freq: f32, velocity: f32) {
        self.base_hz = freq.max(1.0);
        self.set_freq(self.base_hz);
        let period = self.fs / self.base_hz;
        // Short contact: a fraction of a period, so the burst is impulsive.
        self.exc_rem = (period * 0.45).max(2.0) as u32;
        self.exc_len = self.exc_rem.max(1);
        let v = self.humanize_note(velocity);
        self.exc_amp = v;
        // Struck close to the bridge → a bright comb, but eased so the mezrab
        // reads as a woody tap rather than a sharp click.
        self.comb.set_delay((0.09 * period).max(1.0));
        self.comb_amt = 0.58;
        self.vel_bright = v;
    }
    /// True when any per-sample pitch modulation is in flight.
    #[inline]
    fn modulating(&self) -> bool {
        self.vib_on || (self.bend_cur - self.bend_cents).abs() > 1e-3
    }
    #[inline]
    fn tick(&mut self) -> f32 {
        let s = self.delay.last_out();
        // Apply glide + vibrato to the pitch when the left hand is active.
        if self.modulating() {
            self.bend_cur += (self.bend_cents - self.bend_cur) * self.glide;
            let vib = if self.vib_on {
                self.vib_phase += core::f32::consts::TAU * self.vib_rate / self.fs;
                if self.vib_phase >= core::f32::consts::TAU {
                    self.vib_phase -= core::f32::consts::TAU;
                }
                // Nonghyeon pushes the string *up* from the base pitch and lets
                // it back — the player presses behind the bridge, which can only
                // raise pitch. So the modulation is one-sided (0 → +depth),
                // never symmetric around the note. A raised cosine gives that
                // smooth push-and-release.
                self.vib_depth * 0.5 * (1.0 - mathf::cos(self.vib_phase))
            } else {
                0.0
            };
            let eff = self.base_hz * mathf::exp2((self.bend_cur + vib) / 1200.0);
            self.set_freq(eff);
        }
        // Loop loss filter (one-pole low-pass). A harder pluck opens it briefly,
        // injecting extra HF the way a real hard pluck does — kept gentle so the
        // attack stays round rather than zingy.
        let damp = (self.damp * (1.0 - 0.30 * self.vel_bright)).clamp(0.0, 0.995);
        self.vel_bright *= 0.9994;
        self.lp_y1 = (1.0 - damp) * s + damp * self.lp_y1;
        let mut fed = self.decay * self.lp_y1;
        // String-stiffness dispersion: cascade of second-order allpass sections.
        for i in 0..self.disp_stages {
            fed = self.disp[i].process(fed);
        }
        // Gentle loop saturation for harmonic warmth (unity for small signals).
        if self.sat != 0.0 {
            let k = 1.0 + self.sat;
            fed = mathf::tanh(k * fed) / k;
        }
        let exc = if self.onset > 0 {
            // Waiting out the per-string strike offset — no excitation yet.
            self.onset -= 1;
            0.0
        } else if self.exc_rem > 0 {
            self.exc_rem -= 1;
            // Raised-cosine fade-in over the first ~1.2 ms of the burst so the
            // note starts rounded, not as a hard transient click.
            let done = self.exc_len.saturating_sub(self.exc_rem);
            let ramp = ((0.0012 * self.fs) as u32).max(1);
            let env = if done < ramp {
                0.5 - 0.5 * mathf::cos(core::f32::consts::PI * done as f32 / ramp as f32)
            } else {
                1.0
            };
            let n = self.white() * self.exc_amp * env;
            n - self.comb_amt * self.comb.tick(n)
        } else {
            0.0
        };
        // Sympathetic bridge coupling (consumed once).
        let ext = self.ext_in;
        self.ext_in = 0.0;
        self.delay.tick(fed + exc + ext);
        s
    }
    #[inline]
    fn active(&self) -> bool {
        self.exc_rem > 0
            || self.onset > 0
            || self.lp_y1.abs() > 1e-5
            || self.delay.last_out().abs() > 1e-5
    }
}

/// The **çifteli**: a drone string + a melody string sharing one bright body.
pub struct Cifteli {
    drone: PluckString,
    melody: PluckString,
    body: [Reso; 2],
    drone_hz: f32,
    body_mix: f32,
    /// Melody → drone sympathetic coupling depth (shared bridge).
    couple: f32,
}

impl Cifteli {
    pub fn new(fs: f32) -> Self {
        let mut c = Cifteli {
            drone: PluckString::new(fs, 0x1111),
            melody: PluckString::new(fs, 0x2222),
            // A small, bright boxy wooden body.
            body: [Reso::new(430.0, 12.0, 0.8, fs), Reso::new(1150.0, 9.0, 0.5, fs)],
            drone_hz: 196.0,
            body_mix: 0.15,
            // Light sympathetic halo: enough to set the drone ringing when the
            // melody is plucked, gentle enough never to run away.
            couple: 0.02,
        };
        // Tune the drone loop up front so it can resonate sympathetically even
        // before it is ever plucked.
        c.drone.base_hz = c.drone_hz;
        c.drone.set_freq(c.drone_hz);
        c
    }

    /// Tune the drone string (Hz).
    pub fn set_drone_hz(&mut self, hz: f32) {
        self.drone_hz = hz.max(1.0);
        // Keep the drone loop tuned so the sympathetic coupling rings true.
        self.drone.base_hz = self.drone_hz;
        self.drone.set_freq(self.drone_hz);
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

    /// Per-note humanization (0 = mechanical, 1 = loose): velocity + micro-timing
    /// variation so repeated notes breathe. Pitch is never affected.
    pub fn set_humanize(&mut self, amount: f32) {
        self.drone.set_humanize(amount);
        self.melody.set_humanize(amount);
    }

    /// Whether anything is still ringing.
    pub fn active(&self) -> bool {
        self.drone.active() || self.melody.active()
    }

    /// One (near-mono) stereo sample. A çifteli is one small instrument — both
    /// strings radiate from the same point — so the output is centred, with only
    /// a whisper of width between the strings.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        // The melody and drone share a bridge: a little of the melody string's
        // motion drives the drone loop, so plucking the melody sets the drone
        // humming sympathetically — the çifteli's characteristic droning halo.
        let m = self.melody.tick();
        self.drone.inject(self.couple * m);
        let d = self.drone.tick();
        let mix = d + m;
        let mut b = 0.0;
        for r in &mut self.body {
            b += r.tick(mix);
        }
        let center = (mix + self.body_mix * b) * 1.3;
        // A tiny inter-string spread (±5%) for life, not a hard pan.
        let spread = 0.05 * (d - m) * 1.3;
        (center - spread, center + spread)
    }
}

/// The **sanjo gayageum**: a Korean 12-string plucked zither. Long silk strings
/// over a paulownia-wood body give a warm, singing sustain; the right hand
/// plucks while the left presses behind the movable bridges to bend and shake
/// the pitch — *nonghyeon*, the deep, vocal vibrato that is the soul of the tone.
///
/// A modular voice maps onto that as a small **pool of ringing strings**: each
/// `pluck` allocates the next string (round-robin) so notes overlap and bloom
/// into one another, and the left-hand vibrato/bend is applied to whichever
/// string was plucked most recently — exactly what a player's left hand does.
pub struct Gayageum {
    strings: Vec<PluckString>,
    body: [Reso; 3],
    body_mix: f32,
    next: usize,
    /// The string the "left hand" is currently working (last plucked).
    active: usize,
    /// Current nonghyeon vibrato settings applied to the active string.
    vib_rate: f32,
    vib_depth: f32,
    /// Stereo spread across the string field (0 = mono, 1 = full).
    width: f32,
}

impl Gayageum {
    /// Build a gayageum voice. `voices` is the polyphony (how many strings can
    /// ring at once); a real sanjo phrase overlaps ~4–6 notes.
    pub fn new(fs: f32) -> Self {
        Self::with_voices(fs, 6)
    }

    /// Build with an explicit polyphony.
    pub fn with_voices(fs: f32, voices: usize) -> Self {
        let voices = voices.max(1);
        let mut strings = Vec::with_capacity(voices);
        for i in 0..voices {
            let mut s = PluckString::new(fs, 0x9E37u32.wrapping_mul(i as u32 + 1) ^ 0xA53C);
            // Silk strings over paulownia: warm, long, a touch of stiffness.
            s.damp = 0.42;
            s.decay = 0.9975;
            // A whisper of dispersion (silk is only slightly stiff).
            s.set_dispersion(0.55, 1600.0, 1);
            s.sat = 0.15;
            s.glide = 0.0025;
            s.vib_rate = 5.5;
            strings.push(s);
        }
        Gayageum {
            strings,
            // Paulownia body: warm, woody low-mid resonances (lower than the
            // bright çifteli box).
            body: [
                Reso::new(180.0, 8.0, 0.8, fs),
                Reso::new(320.0, 10.0, 0.6, fs),
                Reso::new(700.0, 7.0, 0.35, fs),
            ],
            body_mix: 0.16,
            next: 0,
            active: 0,
            vib_rate: 5.5,
            vib_depth: 40.0,
            width: 0.5,
        }
    }

    /// Pluck a note (Hz — feed it already quantized from the upstream microtonal
    /// quantizer). Allocates the next string in the pool and hands it the left
    /// hand (nonghyeon vibrato + any active bend follow this note).
    pub fn pluck(&mut self, hz: f32, velocity: f32) {
        // Retire the previous string's vibrato — the left hand has moved on.
        let prev = self.active;
        self.strings[prev].vib_on = false;

        let i = self.next;
        self.next = (self.next + 1) % self.strings.len();
        self.active = i;
        let s = &mut self.strings[i];
        s.pluck(hz, velocity);
        // Reset bend so the new note starts at pitch; the left hand can bend it.
        s.bend_cents = 0.0;
        s.bend_cur = 0.0;
        s.vib_rate = self.vib_rate;
        s.vib_depth = self.vib_depth;
        s.vib_on = self.vib_depth > 0.0;
    }

    /// Set the *nonghyeon* vibrato applied to the sounding note. `depth_cents`
    /// is deep for the gayageum — 30–80 cents is idiomatic (a real player's
    /// shake is wide and vocal). `rate_hz` ≈ 4–7 Hz.
    pub fn set_vibrato(&mut self, rate_hz: f32, depth_cents: f32) {
        self.vib_rate = rate_hz.max(0.0);
        self.vib_depth = depth_cents.max(0.0);
        // Update the currently sounding string immediately.
        if let Some(s) = self.strings.get_mut(self.active) {
            s.vib_rate = self.vib_rate;
            s.vib_depth = self.vib_depth;
            s.vib_on = self.vib_depth > 0.0;
        }
    }

    /// Bend the sounding note toward `cents` (glides in — the pressed-string
    /// pitch push). Positive bends up; the left hand behind the bridge only
    /// raises pitch, so callers typically pass ≥ 0.
    pub fn set_bend(&mut self, cents: f32) {
        if let Some(s) = self.strings.get_mut(self.active) {
            s.bend_cents = cents;
        }
    }

    /// Brightness of the strings: 0 = dark/mellow, 1 = bright and singing.
    pub fn set_brightness(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        let damp = 0.6 - 0.4 * a;
        for s in &mut self.strings {
            s.damp = damp;
        }
    }

    /// Sustain of the strings: 0 = short, 1 = long singing ring.
    pub fn set_sustain(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        let decay = 0.992 + 0.0072 * a;
        for s in &mut self.strings {
            s.decay = decay;
        }
    }

    /// Stereo spread of the string field (0 = mono, 1 = wide).
    pub fn set_width(&mut self, amount: f32) {
        self.width = amount.clamp(0.0, 1.0);
    }

    /// Per-note humanization (0 = mechanical, 1 = loose). Pitch-neutral.
    pub fn set_humanize(&mut self, amount: f32) {
        for s in &mut self.strings {
            s.set_humanize(amount);
        }
    }

    /// Whether any string is still ringing.
    pub fn active(&self) -> bool {
        self.strings.iter().any(|s| s.active())
    }

    /// One stereo sample. Strings are laid gently left-to-right across the
    /// player's lap; the paulownia body colours their sum, centred.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let n = self.strings.len();
        let mut mono = 0.0;
        let mut l = 0.0;
        let mut r = 0.0;
        for (i, s) in self.strings.iter_mut().enumerate() {
            let x = s.tick();
            mono += x;
            // Equal-ish pan by string index, scaled by width.
            let pos = if n > 1 { i as f32 / (n - 1) as f32 - 0.5 } else { 0.0 };
            let pan = pos * self.width;
            l += x * (0.5 - pan);
            r += x * (0.5 + pan);
        }
        let mut b = 0.0;
        for res in &mut self.body {
            b += res.tick(mono);
        }
        let body = self.body_mix * b;
        // 1/sqrt(n) makeup so polyphony doesn't pile up level.
        let g = 1.6 / mathf::sqrt(n as f32);
        ((l + body) * g, (r + body) * g)
    }
}

/// The **basitar**: a two-string bass/guitar hybrid — a guitar stripped to two
/// heavy-gauge strings tuned a fifth apart (Mark Sandman's low-slung invention,
/// famous in the hands of Chris Ballew). It plays raw power-chord basslines: a
/// low root string plus a string a fifth up, run through a pickup and a bit of
/// amp grind.
///
/// The two strings are heavy — modeled with strong stiffness (audible
/// inharmonic "clank"), long sustain, and a picked-near-the-bridge attack — and
/// the output goes through an overdrive stage and a single-coil-style presence
/// peak into an amp-like low-pass. Not an acoustic body: the "tone" is the
/// pickup and the amp.
pub struct Basitar {
    low: PluckString,
    high: PluckString,
    /// Interval (semitones) the high string sits above the low. Classic basitar
    /// tuning is a fifth (7).
    interval: f32,
    /// Overdrive / amp grind (0 = clean).
    drive: f32,
    /// Single-coil presence peak.
    presence: Reso,
    /// Amp-style output low-pass (one-pole) state + coefficient.
    amp_lp: f32,
    amp_pole: f32,
    /// Stereo width (electric → mostly mono).
    width: f32,
}

impl Basitar {
    pub fn new(fs: f32) -> Self {
        let mut low = PluckString::new(fs, 0x0B17);
        let mut high = PluckString::new(fs, 0x0B18);
        for s in [&mut low, &mut high] {
            // Heavy strings: strong stiffness (audible inharmonic clank — high
            // partials pulled sharp), long sustain, fairly dark loop with a
            // bright picked attack. A first-order allpass can't disperse a bass
            // string (its phase is flat down low), so use a real second-order
            // dispersion cascade whose group-delay bump sits at ~450 Hz: every
            // partial above it is stretched sharp, like a real wound low string.
            s.damp = 0.38;
            s.decay = 0.978;
            // Dispersion at ~450 Hz stretches the high partials sharp for the
            // clank — eased to 0.72 so it reads as heavy-string character rather
            // than an aggressive metallic edge.
            s.set_dispersion(0.72, 450.0, 2);
            s.sat = 0.0; // grind lives in the amp stage, not the string loop
        }
        Basitar {
            low,
            high,
            interval: 7.0,
            drive: 0.20,
            presence: Reso::new(2400.0, 2.2, 0.5, fs),
            amp_lp: 0.0,
            // ~4.5 kHz amp roll-off.
            amp_pole: mathf::exp(-core::f32::consts::TAU * 4500.0 / fs),
            width: 0.12,
        }
    }

    /// Interval (semitones) between the two strings; classic tuning is a fifth.
    pub fn set_interval(&mut self, semitones: f32) {
        self.interval = semitones;
    }

    /// Pluck only the low (root) string.
    pub fn pluck_low(&mut self, hz: f32, velocity: f32) {
        self.low.pluck(hz, velocity);
    }

    /// Pluck only the high string.
    pub fn pluck_high(&mut self, hz: f32, velocity: f32) {
        self.high.pluck(hz, velocity);
    }

    /// Strum the power chord: root on the low string, a fifth (its `interval`)
    /// up on the high string — the basitar's signature. `hz` is the root (feed
    /// it already quantized). A tiny stagger between the strings mimics the pick
    /// sweeping across both.
    pub fn pluck(&mut self, hz: f32, velocity: f32) {
        let hz = hz.max(1.0);
        self.low.pluck(hz, velocity);
        self.high.pluck(hz * mathf::exp2(self.interval / 12.0), velocity * 0.95);
        // Delay the high string's onset slightly — the pick sweeping across the
        // two strings (composes with any humanization offset).
        self.high.set_onset((0.0015 * self.high.fs) as u32);
    }

    /// Amp grind: 0 = clean, 1 = heavily overdriven.
    pub fn set_drive(&mut self, amount: f32) {
        self.drive = amount.clamp(0.0, 1.0);
    }

    /// Per-note humanization (0 = mechanical, 1 = loose). Pitch-neutral.
    pub fn set_humanize(&mut self, amount: f32) {
        self.low.set_humanize(amount);
        self.high.set_humanize(amount);
    }

    /// Brightness of both strings: 0 = dark/dubby, 1 = bright and clanky.
    pub fn set_brightness(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        let damp = 0.55 - 0.35 * a;
        self.low.damp = damp;
        self.high.damp = damp;
    }

    /// Sustain of both strings: 0 = short/muted, 1 = long ringing.
    pub fn set_sustain(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        // Heavy strings still ring, but for a musical few seconds — not the ~30 s
        // the old range gave a low string. At 82 Hz this spans ≈1.8 s … 5.5 s.
        let decay = 0.955 + 0.030 * a;
        self.low.decay = decay;
        self.high.decay = decay;
    }

    /// Stereo width (0 = mono, 1 = strings spread L/R).
    pub fn set_width(&mut self, amount: f32) {
        self.width = amount.clamp(0.0, 1.0);
    }

    /// Whether either string is still ringing.
    pub fn active(&self) -> bool {
        self.low.active() || self.high.active()
    }

    /// One stereo sample: strings → overdrive → presence → amp low-pass.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let lo = self.low.tick();
        let hi = self.high.tick();
        let mut x = lo + hi;
        // Amp grind: asymmetric-ish soft clip driven by `drive`.
        if self.drive > 0.0 {
            let k = 1.0 + 6.0 * self.drive;
            x = mathf::tanh(k * x) / mathf::tanh(k).max(1e-3);
        }
        // Single-coil presence peak (parallel), then amp roll-off.
        x += 0.5 * self.presence.tick(x);
        self.amp_lp = (1.0 - self.amp_pole) * x + self.amp_pole * self.amp_lp;
        let out = self.amp_lp * 0.8;
        // Mostly mono; a hair of string spread.
        let spread = 0.5 * self.width * (lo - hi);
        (out - spread, out + spread)
    }
}

/// Number of near-unison strings per santur course. Real santurs run 3–4
/// strings to a note; their slight detuning is what gives the shimmer.
const SANTUR_STRINGS_PER_COURSE: usize = 3;

/// One santur course: a handful of steel strings tuned to (almost) the same
/// pitch and struck together. The tiny detuning between them beats, producing
/// the instrument's signature shimmering chorus.
/// Per-string detune pattern within a course, as a fraction of the shimmer
/// width. Deliberately **asymmetric** (not ±spread/2): the two string-pairs then
/// beat at unrelated rates, so the chorus breathes irregularly instead of
/// pulsing as one coherent, mechanical wobble.
const SANTUR_DETUNE_PATTERN: [f32; SANTUR_STRINGS_PER_COURSE] = [-0.62, 0.14, 0.58];
/// Per-string loss trim: each string's (1 − decay) is scaled a few percent, so
/// no two strings of a course decay at quite the same rate.
const SANTUR_DECAY_TRIM: [f32; SANTUR_STRINGS_PER_COURSE] = [1.0, 0.965, 1.04];
/// Per-string strike micro-timing (samples): a mezrab does not contact every
/// string of a course on the same sample.
const SANTUR_ONSET: [u32; SANTUR_STRINGS_PER_COURSE] = [0, 13, 5];

struct Course {
    strings: [PluckString; SANTUR_STRINGS_PER_COURSE],
    /// Base loop gain (before the per-string trim) most recently requested.
    base_decay: f32,
}
impl Course {
    fn new(fs: f32, seed: u32) -> Self {
        // Distinct seeds so the noise bursts (and thus the beating) decorrelate.
        let strings = core::array::from_fn(|k| {
            let mut s = PluckString::new(fs, seed ^ (0x2971u32.wrapping_mul(k as u32 + 1)));
            // Steel strings with a long, ringing shimmer and a touch of
            // stiffness (the santur's high courses are audibly inharmonic, and
            // steel reads bright — partials pulled a hair sharp, not flat).
            // damp eased a little from the brightest setting so the struck tone
            // is warm and shimmering rather than glassy/intense.
            s.damp = 0.34;
            s.set_dispersion(0.72, 1500.0, 1);
            s
        });
        let mut c = Course { strings, base_decay: 0.986 };
        c.set_decay(0.986);
        c
    }
    /// Apply a base loop gain to every string, scaled by its per-string trim so
    /// the course's strings ring for slightly different lengths.
    fn set_decay(&mut self, base: f32) {
        self.base_decay = base;
        for (k, s) in self.strings.iter_mut().enumerate() {
            s.decay = (1.0 - (1.0 - base) * SANTUR_DECAY_TRIM[k]).clamp(0.0, 0.99999);
        }
    }
    fn active(&self) -> bool {
        self.strings.iter().any(|s| s.active())
    }
}

/// The **santur**: the Persian trapezoidal hammered dulcimer. Each note is a
/// *course* of several steel strings struck together by a light wooden mallet
/// (*mezrab*); the strings are tuned a few cents apart so they beat into a
/// shimmering chorus, and the trapezoidal soundboard colours the whole. Played
/// fast, the overlapping courses cascade — the sound the santur is loved for.
///
/// A modular voice maps onto it as a **pool of courses**: each strike allocates
/// the next course (round-robin) so notes ring on and bloom into one another,
/// exactly like a player's rapid two-mallet tremolo. Feed it already-quantized
/// pitches (e.g. from a dastgah `.scl` via the [`puget_dsp`] quantizer).
pub struct Santur {
    courses: Vec<Course>,
    body: [Reso; 3],
    body_mix: f32,
    next: usize,
    /// Unison spread within a course, in cents (the shimmer).
    detune_cents: f32,
    width: f32,
}

impl Santur {
    /// Build a santur voice. `courses` is the polyphony (how many notes can ring
    /// at once); a fast santur passage overlaps many.
    pub fn new(fs: f32) -> Self {
        Self::with_courses(fs, 6)
    }

    /// Build with an explicit polyphony.
    pub fn with_courses(fs: f32, courses: usize) -> Self {
        let courses_n = courses.max(1);
        let mut courses = Vec::with_capacity(courses_n);
        for i in 0..courses_n {
            courses.push(Course::new(fs, 0x5A17u32.wrapping_mul(i as u32 + 1)));
        }
        Santur {
            courses,
            // Trapezoidal walnut soundboard: bright, woody, with an airy top.
            body: [
                Reso::new(210.0, 7.0, 0.7, fs),
                Reso::new(560.0, 9.0, 0.5, fs),
                Reso::new(1600.0, 6.0, 0.3, fs),
            ],
            body_mix: 0.17,
            next: 0,
            detune_cents: 5.0,
            width: 0.4,
        }
    }

    /// Strike a note (Hz — feed it already quantized). Allocates the next course
    /// and hits all of its strings together, each detuned a few cents around the
    /// target so the course shimmers.
    pub fn strike(&mut self, hz: f32, velocity: f32) {
        let hz = hz.max(1.0);
        let i = self.next;
        self.next = (self.next + 1) % self.courses.len();
        for (k, s) in self.courses[i].strings.iter_mut().enumerate() {
            // Asymmetric detune → the string-pairs beat at unrelated rates for an
            // irregular, breathing shimmer (not a coherent mechanical chorus).
            let cents = SANTUR_DETUNE_PATTERN[k] * self.detune_cents;
            s.strike(hz * mathf::exp2(cents / 1200.0), velocity);
            // A mezrab doesn't hit every string on the same sample.
            s.set_onset(SANTUR_ONSET[k]);
        }
    }

    /// Unison detune within a course, in cents (0 = dead unison, more = wider,
    /// wetter shimmer). Applies to the next strike.
    pub fn set_shimmer(&mut self, cents: f32) {
        self.detune_cents = cents.max(0.0);
    }

    /// Brightness of the strings: 0 = soft/felt, 1 = bright/brilliant steel.
    pub fn set_brightness(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        let damp = 0.5 - 0.35 * a;
        for c in &mut self.courses {
            for s in &mut c.strings {
                s.damp = damp;
            }
        }
    }

    /// Sustain of the strings: 0 = quickly damped, 1 = long ringing shimmer.
    pub fn set_sustain(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        // Moderate ring — a shimmering few seconds, not the 40 s+ the old range
        // gave the low courses. Each string then gets its small per-string trim.
        let decay = 0.978 + 0.016 * a;
        for c in &mut self.courses {
            c.set_decay(decay);
        }
    }

    /// Stereo spread of the soundboard field (0 = mono, 1 = wide).
    pub fn set_width(&mut self, amount: f32) {
        self.width = amount.clamp(0.0, 1.0);
    }

    /// Per-note humanization (0 = mechanical, 1 = loose): velocity + micro-timing
    /// variation on top of the course's built-in string stagger. Pitch-neutral.
    pub fn set_humanize(&mut self, amount: f32) {
        for c in &mut self.courses {
            for s in &mut c.strings {
                s.set_humanize(amount);
            }
        }
    }

    /// Whether any course is still ringing.
    pub fn active(&self) -> bool {
        self.courses.iter().any(|c| c.active())
    }

    /// One stereo sample. Courses are laid across the trapezoid left-to-right;
    /// the soundboard colours their sum, centred.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let n = self.courses.len();
        let mut mono = 0.0;
        let mut l = 0.0;
        let mut r = 0.0;
        for (i, c) in self.courses.iter_mut().enumerate() {
            let mut cx = 0.0;
            for s in &mut c.strings {
                cx += s.tick();
            }
            mono += cx;
            let pos = if n > 1 { i as f32 / (n - 1) as f32 - 0.5 } else { 0.0 };
            let pan = pos * self.width;
            l += cx * (0.5 - pan);
            r += cx * (0.5 + pan);
        }
        let mut b = 0.0;
        for res in &mut self.body {
            b += res.tick(mono);
        }
        let body = self.body_mix * b;
        // Makeup for the many strings (courses × strings-per-course).
        let g = 1.4 / mathf::sqrt((n * SANTUR_STRINGS_PER_COURSE) as f32);
        ((l + body) * g, (r + body) * g)
    }
}

/// Strings per tar course (a doubled course — two strings to a note).
const TAR_STRINGS_PER_COURSE: usize = 2;
/// A tar course's two strings sit a few cents apart (the paired-string chorus).
const TAR_DETUNE_PATTERN: [f32; TAR_STRINGS_PER_COURSE] = [-0.5, 0.5];

/// One tar course: a pair of strings tuned to (almost) the same pitch and
/// plucked together by the brass plectrum, their slight detuning giving the
/// paired-course shimmer.
struct TarCourse {
    strings: [PluckString; TAR_STRINGS_PER_COURSE],
}
impl TarCourse {
    fn new(fs: f32, seed: u32) -> Self {
        let strings = core::array::from_fn(|k| {
            let mut s = PluckString::new(fs, seed ^ (0x3517u32.wrapping_mul(k as u32 + 1)));
            // Bright bronze/steel strings, long ringing, a touch of stiffness.
            s.damp = 0.40;
            s.decay = 0.9965;
            s.set_dispersion(0.4, 2000.0, 1);
            s
        });
        TarCourse { strings }
    }
    fn active(&self) -> bool {
        self.strings.iter().any(|s| s.active())
    }
}

/// The **tar**: the central long-necked lute of Persian classical music. Its
/// waisted double-bowl is faced not with wood but a stretched **skin membrane**
/// (traditionally a lamb's heart sac), which gives the tar its bright, singing,
/// slightly nasal twang — quite unlike a wooden-bodied oud. The strings run in
/// **doubled courses**, plucked with a brass plectrum (*mezrab*).
///
/// A modular voice maps onto it as a **pool of courses**: each pluck allocates
/// the next course (round-robin) so notes ring on and bloom together, and each
/// course sounds a pair of strings with a tiny detune + plectrum-sweep stagger.
/// The skin-membrane top is modeled as a bright, resonant body (higher and more
/// nasal than the wooden lutes). Feed it already-quantized pitches.
pub struct Tar {
    courses: Vec<TarCourse>,
    body: [Reso; 3],
    body_mix: f32,
    next: usize,
    detune_cents: f32,
    width: f32,
}

impl Tar {
    /// Build a tar voice with default polyphony.
    pub fn new(fs: f32) -> Self {
        Self::with_courses(fs, 6)
    }

    /// Build with an explicit polyphony (overlapping courses).
    pub fn with_courses(fs: f32, courses: usize) -> Self {
        let courses_n = courses.max(1);
        let mut courses = Vec::with_capacity(courses_n);
        for i in 0..courses_n {
            courses.push(TarCourse::new(fs, 0x7A19u32.wrapping_mul(i as u32 + 1)));
        }
        Tar {
            courses,
            // Skin-membrane top: bright, resonant, a little nasal — higher and
            // more sustaining than a wooden soundbox.
            body: [
                Reso::new(320.0, 8.0, 0.7, fs),
                Reso::new(880.0, 6.0, 0.5, fs),
                Reso::new(2400.0, 5.0, 0.35, fs),
            ],
            body_mix: 0.20,
            next: 0,
            detune_cents: 4.0,
            width: 0.35,
        }
    }

    /// Pluck a note (Hz — feed it already quantized). Allocates the next course
    /// and plucks its pair of strings, detuned a few cents with a tiny sweep.
    pub fn pluck(&mut self, hz: f32, velocity: f32) {
        let hz = hz.max(1.0);
        let i = self.next;
        self.next = (self.next + 1) % self.courses.len();
        for (k, s) in self.courses[i].strings.iter_mut().enumerate() {
            let cents = TAR_DETUNE_PATTERN[k] * self.detune_cents;
            s.pluck(hz * mathf::exp2(cents / 1200.0), velocity);
            // The plectrum crosses the two strings a hair apart.
            s.set_onset((k as f32 * 0.0007 * s.fs) as u32);
        }
    }

    /// Detune between the two strings of a course, in cents (the paired-string
    /// chorus). Applies to the next pluck.
    pub fn set_chorus(&mut self, cents: f32) {
        self.detune_cents = cents.max(0.0);
    }

    /// Brightness of the strings: 0 = mellow, 1 = bright and twangy.
    pub fn set_brightness(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        let damp = 0.55 - 0.35 * a;
        for c in &mut self.courses {
            for s in &mut c.strings {
                s.damp = damp;
            }
        }
    }

    /// Sustain of the strings: 0 = short, 1 = long ringing.
    pub fn set_sustain(&mut self, amount: f32) {
        let a = amount.clamp(0.0, 1.0);
        let decay = 0.990 + 0.0085 * a;
        for c in &mut self.courses {
            for s in &mut c.strings {
                s.decay = decay;
            }
        }
    }

    /// Stereo spread of the course field (0 = mono, 1 = wide).
    pub fn set_width(&mut self, amount: f32) {
        self.width = amount.clamp(0.0, 1.0);
    }

    /// Per-note humanization (0 = mechanical, 1 = loose). Pitch-neutral.
    pub fn set_humanize(&mut self, amount: f32) {
        for c in &mut self.courses {
            for s in &mut c.strings {
                s.set_humanize(amount);
            }
        }
    }

    /// Whether any course is still ringing.
    pub fn active(&self) -> bool {
        self.courses.iter().any(|c| c.active())
    }

    /// One stereo sample. Courses are laid across the field; the skin-membrane
    /// top colours their sum.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let n = self.courses.len();
        let mut mono = 0.0;
        let mut l = 0.0;
        let mut r = 0.0;
        for (i, c) in self.courses.iter_mut().enumerate() {
            let mut cx = 0.0;
            for s in &mut c.strings {
                cx += s.tick();
            }
            mono += cx;
            let pos = if n > 1 { i as f32 / (n - 1) as f32 - 0.5 } else { 0.0 };
            let pan = pos * self.width;
            l += cx * (0.5 - pan);
            r += cx * (0.5 + pan);
        }
        let mut b = 0.0;
        for res in &mut self.body {
            b += res.tick(mono);
        }
        let body = self.body_mix * b;
        let g = 1.5 / mathf::sqrt((n * TAR_STRINGS_PER_COURSE) as f32);
        ((l + body) * g, (r + body) * g)
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    /// Magnitude of the DFT at frequency `f` over `buf` (Goertzel).
    fn goertzel(buf: &[f32], fs: f32, f: f32) -> f32 {
        let w = core::f32::consts::TAU * f / fs;
        let cw = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for &x in buf {
            let s0 = x + cw * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        (s1 * s1 + s2 * s2 - cw * s1 * s2).abs().sqrt()
    }

    /// Precise frequency of the spectral peak within ±`win_cents` of `target`,
    /// by a fine Goertzel scan of a post-attack window of `buf`.
    fn peak_hz_near(buf: &[f32], fs: f32, target: f32, win_cents: f32) -> f32 {
        let start = buf.len() / 6;
        let seg = &buf[start..(start + (fs as usize).min(buf.len() - start))];
        let lo = target * 2f32.powf(-win_cents / 1200.0);
        let hi = target * 2f32.powf(win_cents / 1200.0);
        let steps = 600usize;
        let (mut best_f, mut best_m) = (target, f32::MIN);
        for i in 0..=steps {
            let f = lo + (hi - lo) * i as f32 / steps as f32;
            let m = goertzel(seg, fs, f);
            if m > best_m {
                best_m = m;
                best_f = f;
            }
        }
        best_f
    }

    /// Cents of a partial relative to its ideal harmonic `k*f0`.
    fn partial_cents(buf: &[f32], fs: f32, f0: f32, k: u32) -> f32 {
        let target = k as f32 * f0;
        let f = peak_hz_near(buf, fs, target, 90.0);
        1200.0 * (f / target).log2()
    }

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
        // The delay-length compensation now accounts for the loss filter's true
        // phase delay, so the open string lands within a couple of cents.
        // (Autocorrelation is lag-quantized to ~8 cents here, far too coarse for
        // a ±2-cent check, so measure the fundamental spectrally.)
        let fs = 48_000.0;
        let mut c = Cifteli::new(fs);
        c.set_sustain(1.0);
        c.set_humanize(0.0); // measure the nominal pitch, not the humanized jitter
        c.pluck_melody(220.0, 1.0);
        let buf: Vec<f32> = (0..24_000).map(|_| c.process().0).collect();
        let cents = partial_cents(&buf, fs, 220.0, 1);
        assert!(cents.abs() < 2.0, "melody off by {cents:.2} cents");
    }

    #[test]
    fn open_strings_land_within_two_cents() {
        // Every plucked voice's open-string fundamental must tune to within
        // ±2 cents (the tightened compensation target).
        let fs = 48_000.0;

        let mut g = Gayageum::with_voices(fs, 1);
        g.set_sustain(1.0);
        g.set_vibrato(0.0, 0.0);
        g.set_humanize(0.0); // measure nominal pitch (humanize is pitch-neutral)
        g.pluck(220.0, 1.0);
        let gb: Vec<f32> = (0..24_000).map(|_| g.process().0).collect();
        let gc = partial_cents(&gb, fs, 220.0, 1);
        assert!(gc.abs() < 2.0, "gayageum off by {gc:.2} cents");

        let mut b = Basitar::new(fs);
        b.set_drive(0.0);
        b.set_sustain(1.0);
        b.set_humanize(0.0);
        b.pluck_low(110.0, 1.0);
        let bb: Vec<f32> = (0..24_000).map(|_| b.process().0).collect();
        let bc = partial_cents(&bb, fs, 110.0, 1);
        assert!(bc.abs() < 2.0, "basitar low off by {bc:.2} cents");

        let mut s = Santur::with_courses(fs, 1);
        s.set_sustain(1.0);
        s.set_shimmer(0.0);
        s.set_humanize(0.0);
        s.strike(220.0, 1.0);
        let sb: Vec<f32> = (0..24_000).map(|_| s.process().0).collect();
        let sc = partial_cents(&sb, fs, 220.0, 1);
        assert!(sc.abs() < 2.0, "santur course off by {sc:.2} cents");
    }

    #[test]
    fn gayageum_polyphony_rings_and_decays() {
        let fs = 48_000.0;
        let mut g = Gayageum::new(fs);
        g.set_brightness(0.55);
        g.set_sustain(0.7);
        // A little overlapping phrase across several strings.
        for (i, &hz) in [196.0f32, 233.08, 293.66, 349.23, 392.0].iter().enumerate() {
            g.pluck(hz, 0.85);
            let _ = i;
            for _ in 0..6_000 {
                let (l, r) = g.process();
                assert!(l.is_finite() && r.is_finite(), "non-finite");
            }
        }
        let mut peak = 0.0f32;
        for _ in 0..4_000 {
            let (l, r) = g.process();
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak > 0.01, "too quiet: {peak}");
        // Ring out with no more plucks — a plucked zither decays.
        for _ in 0..fs as usize * 8 {
            g.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            let (l, r) = g.process();
            tail = tail.max(l.abs()).max(r.abs());
        }
        assert!(tail < peak, "did not decay: tail {tail} vs peak {peak}");
    }

    #[test]
    fn nonghyeon_vibrato_modulates_pitch() {
        // With a deep vibrato, the instantaneous pitch should swing measurably;
        // with none, it should hold steady. Compare zero-crossing-rate variance.
        fn pitch_swing(vib_cents: f32) -> f32 {
            let fs = 48_000.0;
            let mut g = Gayageum::with_voices(fs, 1);
            g.set_sustain(1.0);
            g.set_vibrato(6.0, vib_cents);
            g.pluck(220.0, 1.0);
            // Skip the attack, then measure the period over successive short
            // windows; a vibrato makes the estimated period wander.
            let buf: Vec<f32> = (0..fs as usize).map(|_| g.process().0).collect();
            let win = (fs / 40.0) as usize; // ~1200 samples
            let mut periods = Vec::new();
            let mut w = fs as usize / 4;
            while w + 2 * win < buf.len() {
                // crude period via autocorrelation peak in [120,400] Hz
                let (lo, hi) = ((fs / 400.0) as usize, (fs / 120.0) as usize);
                let mut best = f32::MIN;
                let mut lag0 = lo;
                for lag in lo..hi {
                    let mut s = 0.0;
                    for i in 0..win {
                        s += buf[w + i] * buf[w + i + lag];
                    }
                    if s > best {
                        best = s;
                        lag0 = lag;
                    }
                }
                periods.push(fs / lag0 as f32);
                w += win;
            }
            let mn = periods.iter().cloned().fold(f32::MAX, f32::min);
            let mx = periods.iter().cloned().fold(f32::MIN, f32::max);
            mx - mn
        }

        let swing_off = pitch_swing(0.0);
        let swing_on = pitch_swing(60.0);
        assert!(
            swing_on > swing_off + 2.0,
            "vibrato did not modulate pitch: off {swing_off:.2} Hz, on {swing_on:.2} Hz"
        );
    }

    #[test]
    fn tar_plucks_ring_and_decay_in_tune() {
        let fs = 48_000.0;
        let mut t = Tar::new(fs);
        t.set_brightness(0.6);
        t.set_sustain(0.7);
        for &hz in &[220.0f32, 246.94, 293.66, 329.63] {
            t.pluck(hz, 0.85);
            for _ in 0..6_000 {
                let (l, r) = t.process();
                assert!(l.is_finite() && r.is_finite(), "non-finite");
            }
        }
        let mut peak = 0.0f32;
        for _ in 0..4_000 {
            let (l, r) = t.process();
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak > 0.01, "too quiet: {peak}");
        for _ in 0..fs as usize * 6 {
            t.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            let (l, r) = t.process();
            tail = tail.max(l.abs()).max(r.abs());
        }
        assert!(tail < peak, "did not decay: tail {tail} vs peak {peak}");

        // A single course lands in tune (humanize off for a clean measurement).
        let mut t = Tar::with_courses(fs, 1);
        t.set_sustain(1.0);
        t.set_chorus(0.0);
        t.set_humanize(0.0);
        t.pluck(220.0, 1.0);
        let buf: Vec<f32> = (0..24_000).map(|_| t.process().0).collect();
        let cents = partial_cents(&buf, fs, 220.0, 1);
        assert!(cents.abs() < 2.0, "tar off by {cents:.2} cents");
    }

    #[test]
    fn santur_strikes_ring_and_decay() {
        let fs = 48_000.0;
        let mut s = Santur::new(fs);
        s.set_brightness(0.6);
        s.set_sustain(0.7);
        // A little cascade across courses.
        for &hz in &[261.63f32, 293.66, 329.63, 392.0, 440.0] {
            s.strike(hz, 0.85);
            for _ in 0..5_000 {
                let (l, r) = s.process();
                assert!(l.is_finite() && r.is_finite(), "non-finite");
            }
        }
        let mut peak = 0.0f32;
        for _ in 0..4_000 {
            let (l, r) = s.process();
            peak = peak.max(l.abs()).max(r.abs());
        }
        assert!(peak > 0.01, "too quiet: {peak}");
        for _ in 0..fs as usize * 8 {
            s.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            let (l, r) = s.process();
            tail = tail.max(l.abs()).max(r.abs());
        }
        assert!(tail < peak, "did not decay: tail {tail} vs peak {peak}");
    }

    #[test]
    fn santur_course_is_in_tune() {
        // A struck course should ring at the requested pitch (the unison spread
        // beats around it but the perceived pitch is the centre).
        let fs = 48_000.0;
        // Dead unison isolates pure tuning from the shimmer beating.
        let mut s = Santur::with_courses(fs, 1);
        s.set_sustain(1.0);
        s.set_shimmer(0.0);
        s.set_humanize(0.0);
        s.strike(220.0, 1.0);
        let buf: Vec<f32> = (0..24_000).map(|_| s.process().0).collect();
        let cents = partial_cents(&buf, fs, 220.0, 1);
        assert!(cents.abs() < 2.0, "course off by {cents:.2} cents");
    }

    #[test]
    fn basitar_high_partials_are_sharp() {
        // A heavy bass string is stiff: its high partials stretch audibly sharp
        // (inharmonic clank). Measure partials 8 and 12 relative to the (in-tune)
        // fundamental and require a real, monotone upward stretch.
        let fs = 48_000.0;
        let mut b = Basitar::new(fs);
        b.set_drive(0.0);
        b.set_sustain(1.0);
        b.set_brightness(0.7);
        b.set_humanize(0.0);
        b.pluck_low(110.0, 1.0);
        let buf: Vec<f32> = (0..40_000).map(|_| b.process().0).collect();
        let c1 = partial_cents(&buf, fs, 110.0, 1);
        let c4 = partial_cents(&buf, fs, 110.0, 4);
        let c8 = partial_cents(&buf, fs, 110.0, 8);
        let c12 = partial_cents(&buf, fs, 110.0, 12);
        assert!(c1.abs() < 2.0, "fundamental not in tune: {c1:.2} cents");
        // Partials climb sharp with harmonic number — the heavy-string clank.
        // (Eased from the most aggressive dispersion for a warmer voice, but the
        // stretch is still clearly audible and monotone.)
        assert!(
            c12 - c1 > 4.0,
            "partial 12 not sharp enough: stretch {:.1} cents (p1 {c1:.1}, p12 {c12:.1})",
            c12 - c1
        );
        assert!(
            c8 - c1 > 2.0,
            "partial 8 not sharp enough: stretch {:.1} cents (p1 {c1:.1}, p8 {c8:.1})",
            c8 - c1
        );
        assert!(c8 > c4 && c4 > c1, "dispersion not monotone: {c1:.1} {c4:.1} {c8:.1}");
    }

    #[test]
    fn santur_shimmer_is_irregular() {
        // The three strings of a course must not sit symmetrically around the
        // pitch with identical decay/timing (that beats coherently — a
        // mechanical chorus). Assert the detune is asymmetric, the pairwise beat
        // rates are all distinct, and the decays and strike onsets differ.
        let fs = 48_000.0;
        let mut s = Santur::with_courses(fs, 1);
        s.set_sustain(0.7);
        s.set_shimmer(5.0);
        s.strike(220.0, 1.0);
        let c = &s.courses[0];
        let f: Vec<f32> = c.strings.iter().map(|st| st.base_hz).collect();

        // Detune is asymmetric: the mean offset is not the centre string.
        let mean = (f[0] + f[1] + f[2]) / 3.0;
        assert!(
            (mean - f[1]).abs() > 0.05,
            "detune looks symmetric: {f:?} (mean {mean:.3})"
        );

        // The three pairwise beat rates must all be meaningfully different.
        let b01 = (f[0] - f[1]).abs();
        let b12 = (f[1] - f[2]).abs();
        let b02 = (f[0] - f[2]).abs();
        for (x, y, tag) in [(b01, b12, "01/12"), (b01, b02, "01/02"), (b12, b02, "12/02")] {
            assert!(
                (x - y).abs() > 0.1 * x.max(y),
                "beat rates too close ({tag}): {x:.3} vs {y:.3} Hz"
            );
        }

        // Per-string decays differ (staggered ring-out).
        let d: Vec<f32> = c.strings.iter().map(|st| st.decay).collect();
        assert!(d[0] != d[1] && d[1] != d[2] && d[0] != d[2], "decays identical: {d:?}");

        // Strike micro-timing differs (mezrab doesn't hit all strings at once).
        let o: Vec<u32> = c.strings.iter().map(|st| st.onset).collect();
        assert!(!(o[0] == o[1] && o[1] == o[2]), "onsets identical: {o:?}");
    }

    #[test]
    fn cifteli_drone_rings_sympathetically() {
        // Plucking only the melody string should set the (tuned, un-plucked)
        // drone humming via the shared bridge.
        let fs = 48_000.0;
        let mut c = Cifteli::new(fs);
        c.set_sustain(0.8);
        c.set_drone_hz(196.0);
        // Only the melody is struck.
        c.pluck_melody(392.0, 1.0);
        // Let it develop, then measure the drone string's own energy.
        for _ in 0..24_000 {
            c.process();
        }
        let mut drone_energy = 0.0f32;
        for _ in 0..8_000 {
            c.process();
            drone_energy += c.drone.delay.last_out().powi(2);
        }
        assert!(
            drone_energy > 1e-6 && drone_energy.is_finite(),
            "drone did not ring sympathetically: energy {drone_energy:e}"
        );
    }

    #[test]
    fn basitar_power_chord_is_a_fifth() {
        // The high string should ring a just-ish fifth (700 cents) above the low.
        let fs = 48_000.0;

        fn pitch(buf: &[f32], fs: f32, lo_hz: f32, hi_hz: f32) -> f32 {
            let (lo, hi) = ((fs / hi_hz) as usize, (fs / lo_hz) as usize);
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
            fs / lag0 as f32
        }

        // Low string alone.
        let mut b = Basitar::new(fs);
        b.set_drive(0.0);
        b.set_sustain(1.0);
        b.pluck_low(110.0, 1.0);
        let low: Vec<f32> = (0..16_384).map(|_| b.process().0).collect();
        let f_low = pitch(&low, fs, 80.0, 200.0);

        // High string alone (a fifth up = 7 semitones).
        let mut b = Basitar::new(fs);
        b.set_drive(0.0);
        b.set_sustain(1.0);
        b.pluck_high(110.0 * 2f32.powf(7.0 / 12.0), 1.0);
        let high: Vec<f32> = (0..16_384).map(|_| b.process().0).collect();
        let f_high = pitch(&high, fs, 120.0, 260.0);

        let cents = 1200.0 * (f_high / f_low).log2();
        assert!(
            (cents - 700.0).abs() < 30.0,
            "interval off: {cents:.1} cents ({f_low:.1} -> {f_high:.1} Hz)"
        );
    }

    #[test]
    fn basitar_rings_and_decays() {
        let fs = 48_000.0;
        let mut b = Basitar::new(fs);
        b.set_drive(0.4);
        b.set_sustain(0.6);
        b.pluck(82.41, 0.9); // low E power chord
        let mut peak = 0.0f32;
        for i in 0..48_000 {
            let (l, r) = b.process();
            assert!(l.is_finite() && r.is_finite(), "non-finite");
            if i < 4_000 {
                peak = peak.max(l.abs()).max(r.abs());
            }
        }
        assert!(peak > 0.02, "too quiet: {peak}");
        for _ in 0..48_000 * 8 {
            b.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            let (l, r) = b.process();
            tail = tail.max(l.abs()).max(r.abs());
        }
        assert!(tail < peak, "did not decay: tail {tail} vs peak {peak}");
    }
}
