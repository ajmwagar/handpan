//! # plucked-core
//!
//! Plucked-string physical modeling for lutes, zithers and electric strings:
//! - the **[`Cifteli`]** — the two-string Albanian long-neck lute (a fixed
//!   **drone** string plus a fretted **melody** string),
//! - the **[`Gayageum`]** — the Korean sanjo zither: a pool of long silk strings
//!   with a warm paulownia body and deep left-hand *nonghyeon* (vibrato + bends),
//! - the **[`Basitar`]** — the two-string bass/guitar hybrid: heavy strings
//!   tuned a fifth apart through a pickup and amp grind (power-chord basslines).
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
    /// String-stiffness allpass coefficient (negative = partials stretched
    /// sharp); 0 disables the allpass entirely.
    stiff: f32,
    ap_x1: f32,
    ap_y1: f32,
    /// Loop saturation drive (0 = clean).
    sat: f32,
    /// Noise-burst excitation state.
    rng: u32,
    exc_rem: u32,
    exc_amp: f32,
    /// Pluck-position comb: a short delay whose output is subtracted from the
    /// excitation, plus its depth (0 = off).
    comb: Delay,
    comb_amt: f32,
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
            stiff: 0.0,
            ap_x1: 0.0,
            ap_y1: 0.0,
            sat: 0.0,
            rng: 0x9E37_79B9 ^ seed,
            exc_rem: 0,
            exc_amp: 0.0,
            comb: Delay::new(max),
            comb_amt: 0.0,
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
    fn set_freq(&mut self, freq: f32) {
        let raw = self.fs / freq;
        // A first-order allpass (a + z⁻¹)/(1 + a·z⁻¹) adds frequency-dependent
        // phase delay; subtract its value at the fundamental so the string still
        // tunes to `freq`. (Only when the allpass is actually in the loop.)
        let ap_pd = if self.stiff != 0.0 {
            let a = self.stiff;
            let w = core::f32::consts::TAU * freq / self.fs;
            1.0 + (2.0 / w) * mathf::atan((a * mathf::sin(w)) / (1.0 + a * mathf::cos(w)))
        } else {
            0.0
        };
        // Subtract the one-pole loss-filter (~0.5) + cache (1) + allpass delay.
        self.delay.set_delay((raw - 1.5 - ap_pd).max(2.0));
    }
    /// Pluck: excite the loop with a comb-shaped noise burst ~one period long.
    fn pluck(&mut self, freq: f32, velocity: f32) {
        self.base_hz = freq.max(1.0);
        self.set_freq(self.base_hz);
        let period = self.fs / self.base_hz;
        self.exc_rem = period as u32;
        self.exc_amp = velocity.clamp(0.0, 1.0);
        // Pluck a fraction β along the string → comb notch every 1/β harmonics.
        // β ≈ 0.13 (near the bridge) gives the bright, slightly hollow lute tone.
        self.comb.set_delay((0.13 * period).max(1.0));
        self.comb_amt = 0.9;
        self.vel_bright = self.exc_amp;
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
                self.vib_depth * mathf::sin(self.vib_phase)
            } else {
                0.0
            };
            let eff = self.base_hz * mathf::exp2((self.bend_cur + vib) / 1200.0);
            self.set_freq(eff);
        }
        // Loop loss filter (one-pole low-pass). A harder pluck opens it briefly,
        // injecting extra HF the way a real hard pluck does.
        let damp = (self.damp * (1.0 - 0.45 * self.vel_bright)).clamp(0.0, 0.995);
        self.vel_bright *= 0.9994;
        self.lp_y1 = (1.0 - damp) * s + damp * self.lp_y1;
        let mut fed = self.decay * self.lp_y1;
        // String-stiffness dispersion allpass.
        if self.stiff != 0.0 {
            let y = self.stiff * fed + self.ap_x1 - self.stiff * self.ap_y1;
            self.ap_x1 = fed;
            self.ap_y1 = y;
            fed = y;
        }
        // Gentle loop saturation for harmonic warmth (unity for small signals).
        if self.sat != 0.0 {
            let k = 1.0 + self.sat;
            fed = mathf::tanh(k * fed) / k;
        }
        let exc = if self.exc_rem > 0 {
            self.exc_rem -= 1;
            let n = self.white() * self.exc_amp;
            n - self.comb_amt * self.comb.tick(n)
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

    /// One (near-mono) stereo sample. A çifteli is one small instrument — both
    /// strings radiate from the same point — so the output is centred, with only
    /// a whisper of width between the strings.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let d = self.drone.tick();
        let m = self.melody.tick();
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
            s.stiff = -0.12;
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
            // Heavy strings: strong stiffness (inharmonic clank), long sustain,
            // fairly dark loop with a bright picked attack.
            s.damp = 0.38;
            s.decay = 0.9985;
            s.stiff = -0.24;
            s.sat = 0.0; // grind lives in the amp stage, not the string loop
        }
        Basitar {
            low,
            high,
            interval: 7.0,
            drive: 0.25,
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
        // Offset the high string's excitation slightly (pick sweep).
        self.high.exc_rem = self.high.exc_rem.saturating_add((0.0015 * self.high.fs) as u32);
    }

    /// Amp grind: 0 = clean, 1 = heavily overdriven.
    pub fn set_drive(&mut self, amount: f32) {
        self.drive = amount.clamp(0.0, 1.0);
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
        let decay = 0.993 + 0.0065 * a;
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

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]

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
