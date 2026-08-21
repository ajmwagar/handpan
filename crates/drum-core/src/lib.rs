//! # drum-core
//!
//! Membrane-drum physical modeling. Three voices share one circular-membrane
//! engine:
//! - the **[`Dohol`]** — a **bass drum** of the Persian *dohol* / Turkish–Balkan
//!   *davul* family: a big double-headed drum struck with a heavy beater (the
//!   deep boom) and a thin switch (the sharp crack); retuned it covers the
//!   davul, taiko, or a kit bass drum;
//! - the **[`Tombak`]** (*zarb*) — the Persian goblet hand drum, the lead voice
//!   of the classical percussion: a pitched, resonant *tom* at the centre, a
//!   dry bright *bak* at the rim, and light *finger* taps for the rapid rolls.
//! - the **[`Daf`]** — the large Persian frame drum ringed on the inside with
//!   loose metal jingles: a deep open-frame *dum* at the centre and a rim *tak*,
//!   both setting the internal rings shimmering, plus a hand-`shake` that rattles
//!   the jingles on their own. The frame head is the same modal membrane; the
//!   jingles are a cluster of high, inharmonic, fast-decaying metallic rings.
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

/// A 2-pole resonator — one wooden-body mode of the tombak's goblet chamber.
#[derive(Default, Clone, Copy)]
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

/// The **tombak** (*zarb*): the Persian goblet hand drum — the lead voice of the
/// classical percussion. Its wine-goblet wooden body carries a single head that
/// the player works with extraordinary finger detail. Three core strokes:
/// - **tom** — a full-hand hit near the centre: a deep, resonant, pitched tone
///   (the body chamber rings);
/// - **bak** — a fingertip at the rim: a sharp, dry, bright slap;
/// - **finger** — a light articulate tap, the basis of the rapid *riz* rolls.
///
/// The head is the same 2D circular-membrane modal bank as the [`Dohol`], but
/// tuned higher and more pitched, with far less tension pitch-drop (a hand, not
/// a heavy beater) and a resonant goblet **body** colouring the tone.
pub struct Tombak {
    fs: f32,
    fund: f32,
    modes: [ModeState; MEMBRANE.len()],
    glide: f32,
    glide_coef: f32,
    pitch_drop: f32,
    rng: Rng,
    // Fingertip/slap noise transient.
    noise_rem: u32,
    noise_amp: f32,
    noise_lp: f32,
    noise_coef: f32,
    // Goblet wooden body.
    body: [Reso; 2],
    body_mix: f32,
    t60: f32,
    out_lp: f32,
    out_pole: f32,
}

impl Tombak {
    pub fn new(fs: f32) -> Self {
        let mut t = Tombak {
            fs,
            fund: 100.0,
            modes: [ModeState::default(); MEMBRANE.len()],
            glide: 1.0,
            // A hand barely stretches the head — a small, quick pitch-drop.
            glide_coef: 1.0 - mathf::exp(-1.0 / (0.025 * fs)),
            pitch_drop: 0.06,
            rng: Rng(0x2BAD_C0DE),
            noise_rem: 0,
            noise_amp: 0.0,
            noise_lp: 0.0,
            noise_coef: 0.0,
            // Goblet chamber: a woody low thunk + a warm mid.
            body: [Reso::new(190.0, 6.0, 0.7, fs), Reso::new(430.0, 8.0, 0.4, fs)],
            body_mix: 0.22,
            t60: 0.40,
            out_lp: 0.0,
            // Brighter/more open than the bass drum (~7 kHz).
            out_pole: mathf::exp(-core::f32::consts::TAU * 7000.0 / fs),
        };
        t.reset_state();
        t
    }

    fn reset_state(&mut self) {
        for m in &mut self.modes {
            *m = ModeState::default();
        }
    }

    /// Tune the drum — the tom's fundamental (Hz). A tombak sits fairly high for
    /// a hand drum; ~90–130 Hz is typical.
    pub fn set_tune(&mut self, hz: f32) {
        self.fund = hz.clamp(40.0, 400.0);
    }

    /// Base ring length of the tom in seconds (higher modes scale down from it).
    pub fn set_decay(&mut self, seconds: f32) {
        self.t60 = seconds.clamp(0.05, 4.0);
    }

    /// Internal: excite the head. `pos` 0 = centre..1 = rim; `decay_scale` shortens
    /// the ring; `slap`/`bright` set the fingertip-noise level and cutoff; `drop`
    /// scales the (small) tension pitch-drop.
    fn hit(&mut self, velocity: f32, pos: f32, decay_scale: f32, slap: f32, bright: f32, drop: f32) {
        let vel = velocity.clamp(0.0, 1.0);
        let pos = pos.clamp(0.0, 1.0);
        self.glide = 1.0 + self.pitch_drop * drop * (0.5 + 0.5 * vel) * (1.0 - 0.5 * pos);
        for (st, md) in self.modes.iter_mut().zip(MEMBRANE.iter()) {
            let f = self.fund * md.ratio;
            if f >= self.fs * 0.48 {
                st.amp = 0.0;
                continue;
            }
            let m = md.m as f32;
            let pos_w = if md.m == 0 {
                1.0 - 0.55 * pos
            } else {
                (0.2 + pos) * (1.0 + 0.15 * m * pos)
            };
            st.inc = core::f32::consts::TAU * f / self.fs;
            st.phase = (self.rng.next_f() + 1.0) * 0.5 * core::f32::consts::TAU;
            st.amp = vel * md.gain * pos_w;
            let t60 = (self.t60 * md.decay_rel * decay_scale).max(0.01);
            st.dec = mathf::exp(-6.9078 / (t60 * self.fs));
        }
        self.noise_rem = (0.008 * self.fs) as u32;
        self.noise_amp = vel * slap;
        self.noise_coef = 1.0 - mathf::exp(-core::f32::consts::TAU * bright / self.fs);
    }

    /// **Tom**: a full-hand hit near the centre — the deep, resonant, pitched
    /// bass tone (the goblet body rings).
    pub fn tom(&mut self, velocity: f32) {
        self.hit(velocity, 0.12, 1.0, 0.15, 1400.0, 1.0);
    }

    /// **Bak**: a fingertip at the rim — a sharp, dry, bright slap.
    pub fn bak(&mut self, velocity: f32) {
        self.hit(velocity, 0.9, 0.32, 0.95, 6500.0, 0.3);
    }

    /// **Finger**: a light articulate tap — the basis of the rapid *riz* rolls.
    pub fn finger(&mut self, velocity: f32) {
        self.hit(velocity * 0.7, 0.55, 0.5, 0.45, 3800.0, 0.2);
    }

    /// Whether the drum is still sounding.
    pub fn active(&self) -> bool {
        self.noise_rem > 0 || self.modes.iter().any(|m| m.amp > 1e-4)
    }

    /// One stereo sample (a single drum → centred/mono).
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        self.glide += (1.0 - self.glide) * self.glide_coef;

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
        if self.noise_rem > 0 {
            self.noise_rem -= 1;
            let n = self.rng.next_f();
            self.noise_lp += self.noise_coef * (n - self.noise_lp);
            s += self.noise_amp * self.noise_lp;
        }
        // Goblet body colours the head + slap.
        let mut b = 0.0;
        for r in &mut self.body {
            b += r.tick(s);
        }
        let mixed = s + self.body_mix * b;
        self.out_lp += (1.0 - self.out_pole) * (mixed - self.out_lp);
        let out = self.out_lp * 1.2;
        (out, out)
    }
}

/// The internal metal jingles of the [`Daf`]: a small fixed cluster of high,
/// **inharmonic**, fast-decaying ringing partials. Each entry is a frequency
/// ratio to [`JINGLE_BASE`], an excitation gain, and its ring time (T60, s).
/// The ratios are deliberately non-integer — loose rings have no pitch, only a
/// bright metallic shimmer.
const JINGLE: &[(f32, f32, f32)] = &[
    // ratio  gain  t60
    (1.000, 1.00, 0.30),
    (1.347, 0.82, 0.25),
    (1.712, 0.66, 0.21),
    (2.153, 0.52, 0.17),
    (2.629, 0.40, 0.14),
    (3.094, 0.30, 0.11),
];
/// Base frequency of the jingle cluster (Hz) — the lowest ring sits here and the
/// [`JINGLE`] ratios fan the rest up into the bright metallic band.
const JINGLE_BASE: f32 = 3300.0;

/// The **daf**: the large Persian frame drum, its inner rim strung with loose
/// metal rings. Two layers sound together:
/// - a **frame-drum membrane** — the same 2D circular-membrane modal bank as the
///   [`Dohol`] and [`Tombak`], voiced big, low and open with only a slight
///   tension pitch-drop (a hand, not a beater): a deep centre **dum** and a
///   brighter rim **tak**;
/// - a **jingle layer** — a fixed cluster of high, inharmonic, fast-decaying
///   metallic rings ([`JINGLE`]) plus a short band-passed noise chiff. Every
///   strike sets them shimmering, scaled by velocity and by position (a rim
///   *tak* rattles them far more than a centre *dum*), and [`Daf::shake`] rattles
///   them on their own. [`Daf::set_jingle`] dials the layer from a dry frame drum
///   (0) up to full jingles (1).
///
/// The membrane path is rolled off dark like the other drums; the jingle path is
/// summed on top **after** that roll-off, so the rings keep their bright top end.
pub struct Daf {
    fs: f32,
    fund: f32,
    modes: [ModeState; MEMBRANE.len()],
    glide: f32,
    glide_coef: f32,
    pitch_drop: f32,
    rng: Rng,
    // Membrane strike/rim-slap noise transient.
    noise_rem: u32,
    noise_amp: f32,
    noise_lp: f32,
    noise_coef: f32,
    t60: f32,
    // Jingle layer: a cluster of high inharmonic decaying rings...
    jingle: [ModeState; JINGLE.len()],
    // ...a short metallic noise chiff on each excitation, coloured by a high
    // band-pass, giving the rings their shimmering "tsss" edge...
    jn_rem: u32,
    jn_amp: f32,
    jingle_bp: Reso,
    // ...and a sustained shake rattle that keeps re-exciting the rings.
    shake_rem: u32,
    shake_intensity: f32,
    shake_tick: u32,
    jingle_mix: f32,
    // Membrane-path output roll-off.
    out_lp: f32,
    out_pole: f32,
}

impl Daf {
    pub fn new(fs: f32) -> Self {
        let mut d = Daf {
            fs,
            fund: 82.0,
            modes: [ModeState::default(); MEMBRANE.len()],
            glide: 1.0,
            // A frame drum stretches only a little under the hand — a small,
            // quick pitch-drop (~30 ms).
            glide_coef: 1.0 - mathf::exp(-1.0 / (0.030 * fs)),
            pitch_drop: 0.05,
            rng: Rng(0x0DAF_1234),
            noise_rem: 0,
            noise_amp: 0.0,
            noise_lp: 0.0,
            noise_coef: 0.0,
            t60: 0.55,
            jingle: [ModeState::default(); JINGLE.len()],
            jn_rem: 0,
            jn_amp: 0.0,
            // Bright metallic band-pass colouring the jingle noise.
            jingle_bp: Reso::new(6800.0, 2.5, 1.0, fs),
            shake_rem: 0,
            shake_intensity: 0.0,
            shake_tick: 0,
            jingle_mix: 0.7,
            out_lp: 0.0,
            // Membrane rolled off ~4.5 kHz (the frame head has little top of its
            // own — the shimmer comes from the jingles, added after this).
            out_pole: mathf::exp(-core::f32::consts::TAU * 4500.0 / fs),
        };
        for m in &mut d.jingle {
            *m = ModeState::default();
        }
        d
    }

    /// Tune the frame drum — the *dum*'s fundamental (Hz). A big daf sits low and
    /// open; ~70–95 Hz is typical.
    pub fn set_tune(&mut self, hz: f32) {
        self.fund = hz.clamp(40.0, 300.0);
    }

    /// Base ring length of the head in seconds (the fundamental's T60; higher
    /// modes scale down from it). This is the *membrane* decay — the jingles keep
    /// their own fixed, fast metallic ring.
    pub fn set_decay(&mut self, seconds: f32) {
        self.t60 = seconds.clamp(0.05, 6.0);
    }

    /// How much jingle to mix in: 0 = a dry frame drum (jingles silent), 1 = full
    /// shimmering rings on top of every stroke and shake.
    pub fn set_jingle(&mut self, amount: f32) {
        self.jingle_mix = amount.clamp(0.0, 1.0);
    }

    /// Internal: strike the frame head. `pos` 0 = centre..1 = rim; `decay_scale`
    /// shortens the ring; `slap`/`bright` set the strike-noise level and cutoff;
    /// `drop` scales the (small) tension pitch-drop.
    fn strike_head(&mut self, velocity: f32, pos: f32, decay_scale: f32, slap: f32, bright: f32, drop: f32) {
        let vel = velocity.clamp(0.0, 1.0);
        let pos = pos.clamp(0.0, 1.0);
        self.glide = 1.0 + self.pitch_drop * drop * (0.5 + 0.5 * vel) * (1.0 - 0.5 * pos);
        for (st, md) in self.modes.iter_mut().zip(MEMBRANE.iter()) {
            let f = self.fund * md.ratio;
            if f >= self.fs * 0.48 {
                st.amp = 0.0;
                continue;
            }
            let m = md.m as f32;
            let pos_w = if md.m == 0 {
                1.0 - 0.55 * pos
            } else {
                (0.2 + pos) * (1.0 + 0.15 * m * pos)
            };
            st.inc = core::f32::consts::TAU * f / self.fs;
            st.phase = (self.rng.next_f() + 1.0) * 0.5 * core::f32::consts::TAU;
            st.amp = vel * md.gain * pos_w;
            let t60 = (self.t60 * md.decay_rel * decay_scale).max(0.02);
            st.dec = mathf::exp(-6.9078 / (t60 * self.fs));
        }
        self.noise_rem = (0.008 * self.fs) as u32;
        self.noise_amp = vel * slap;
        self.noise_coef = 1.0 - mathf::exp(-core::f32::consts::TAU * bright / self.fs);
    }

    /// Internal: set the jingle cluster ringing with the given excitation
    /// `energy`. When `chiff` is true it also fires the short metallic
    /// noise burst; the shake bed re-excites the rings with `chiff = false`.
    fn excite_jingles(&mut self, energy: f32, chiff: bool) {
        let energy = energy.max(0.0);
        for (st, &(ratio, gain, t60)) in self.jingle.iter_mut().zip(JINGLE.iter()) {
            let f = JINGLE_BASE * ratio;
            if f >= self.fs * 0.48 {
                st.amp = 0.0;
                continue;
            }
            // Loose rings rattle unevenly — jitter each ring's level and phase.
            let jit = 0.7 + 0.3 * (self.rng.next_f() * 0.5 + 0.5);
            st.inc = core::f32::consts::TAU * f / self.fs;
            st.phase = (self.rng.next_f() + 1.0) * 0.5 * core::f32::consts::TAU;
            // Additive so an overlapping strike/shake stacks, capped to stay tame.
            st.amp = (st.amp + energy * gain * jit).min(1.5);
            st.dec = mathf::exp(-6.9078 / (t60 * self.fs));
        }
        if chiff {
            self.jn_rem = (0.012 * self.fs) as u32;
            self.jn_amp = energy;
        }
    }

    /// **Dum**: a deep open hit near the centre — the frame drum's full low tone.
    /// The centre barely disturbs the rings, so only a little jingle shimmer.
    pub fn dum(&mut self, velocity: f32) {
        self.strike_head(velocity, 0.14, 1.0, 0.20, 1600.0, 1.0);
        let vel = velocity.clamp(0.0, 1.0);
        self.excite_jingles(vel * 0.28, true);
    }

    /// **Tak**: a crisp fingers-at-the-rim stroke — dry and bright, and it sets
    /// the metal rings rattling far more than a centre dum.
    pub fn tak(&mut self, velocity: f32) {
        self.strike_head(velocity, 0.92, 0.42, 0.9, 6500.0, 0.3);
        let vel = velocity.clamp(0.0, 1.0);
        self.excite_jingles(vel * 1.0, true);
    }

    /// **Shake**: rattle the jingles on their own (no head strike) — the daf held
    /// up and shaken. `intensity` 0..1 sets how hard and how long the rings rattle.
    pub fn shake(&mut self, intensity: f32) {
        let i = intensity.clamp(0.0, 1.0);
        self.excite_jingles(i * 0.7, true);
        self.shake_intensity = i;
        self.shake_rem = ((0.12 + 0.55 * i) * self.fs) as u32;
        self.shake_tick = 0;
    }

    /// Whether the drum (head or jingles) is still sounding.
    pub fn active(&self) -> bool {
        self.noise_rem > 0
            || self.jn_rem > 0
            || self.shake_rem > 0
            || self.modes.iter().any(|m| m.amp > 1e-4)
            || self.jingle.iter().any(|m| m.amp > 1e-4)
    }

    /// One stereo sample (a single drum → centred/mono).
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        // --- Frame-drum membrane (rolled off dark) ---
        self.glide += (1.0 - self.glide) * self.glide_coef;
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
        if self.noise_rem > 0 {
            self.noise_rem -= 1;
            let n = self.rng.next_f();
            self.noise_lp += self.noise_coef * (n - self.noise_lp);
            s += self.noise_amp * self.noise_lp;
        }
        self.out_lp += (1.0 - self.out_pole) * (s - self.out_lp);
        let membrane = self.out_lp;

        // --- Jingle layer (kept bright, summed on top) ---
        let mut j = 0.0;
        for st in &mut self.jingle {
            if st.amp <= 1e-5 {
                continue;
            }
            j += st.amp * mathf::sin(st.phase);
            st.phase += st.inc;
            if st.phase >= core::f32::consts::TAU {
                st.phase -= core::f32::consts::TAU;
            }
            st.amp *= st.dec;
        }
        // Metallic noise chiff + sustained shake rattle, both coloured by the
        // bright band-pass so they read as ringing metal, not white noise.
        let mut chiff = 0.0;
        if self.jn_rem > 0 {
            self.jn_rem -= 1;
            chiff += self.rng.next_f() * self.jn_amp;
        }
        if self.shake_rem > 0 {
            self.shake_rem -= 1;
            chiff += self.rng.next_f() * self.shake_intensity * 0.5;
            // Periodically re-excite the rings so a shake evolves and rattles
            // rather than ringing one clean chord.
            if self.shake_tick == 0 {
                let e = self.shake_intensity * (0.25 + 0.35 * (self.rng.next_f() * 0.5 + 0.5));
                self.excite_jingles(e, false);
                let rate = 18.0 + 22.0 * self.shake_intensity; // rattles/sec
                self.shake_tick = (self.fs / rate) as u32;
            } else {
                self.shake_tick -= 1;
            }
        }
        j += self.jingle_bp.tick(chiff);

        let out = membrane + self.jingle_mix * j;
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

    #[test]
    fn tombak_strokes_sound_and_decay() {
        let fs = 48_000.0;
        let mut t = Tombak::new(fs);
        t.set_tune(100.0);
        for (i, stroke) in [0u8, 1, 2].iter().enumerate() {
            match stroke {
                0 => t.tom(0.9),
                1 => t.bak(0.9),
                _ => t.finger(0.9),
            }
            let _ = i;
            let mut peak = 0.0f32;
            for _ in 0..3_000 {
                let (l, r) = t.process();
                assert!(l.is_finite() && r.is_finite(), "non-finite");
                peak = peak.max(l.abs());
            }
            assert!(peak > 0.01, "stroke {stroke} too quiet: {peak}");
        }
        // Ring out; a drum decays.
        let mut peak = 0.0f32;
        t.tom(1.0);
        for i in 0..fs as usize * 2 {
            let x = t.process().0.abs();
            if i < 3_000 {
                peak = peak.max(x);
            }
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            tail = tail.max(t.process().0.abs());
        }
        assert!(tail < peak, "did not decay: tail {tail} vs peak {peak}");
    }

    #[test]
    fn bak_is_brighter_and_shorter_than_tom() {
        // The rim bak should be brighter (more HF) and decay faster than the
        // centre tom.
        let fs = 48_000.0;
        fn measure(bak: bool) -> (f32, f32) {
            let fs = 48_000.0;
            let mut t = Tombak::new(fs);
            t.set_tune(100.0);
            t.set_decay(0.4);
            if bak {
                t.bak(0.9);
            } else {
                t.tom(0.9);
            }
            let mut prev = 0.0;
            let (mut hf, mut tot) = (0.0f32, 1e-9f32);
            let mut early = 0.0f32;
            let mut late = 0.0f32;
            for i in 0..12_000 {
                let x = t.process().0;
                let h = x - prev;
                prev = x;
                hf += h * h;
                tot += x * x;
                if i < 1_000 {
                    early = early.max(x.abs());
                }
                if (6_000..7_000).contains(&i) {
                    late = late.max(x.abs());
                }
            }
            (hf / tot, late / early.max(1e-9))
        }
        let (tom_hf, tom_sustain) = measure(false);
        let (bak_hf, bak_sustain) = measure(true);
        assert!(bak_hf > tom_hf, "bak {bak_hf:.4} not brighter than tom {tom_hf:.4}");
        assert!(bak_sustain < tom_sustain, "bak did not decay faster: {bak_sustain:.4} vs {tom_sustain:.4}");
    }

    #[test]
    fn daf_strokes_sound_and_decay() {
        let fs = 48_000.0;
        let mut d = Daf::new(fs);
        d.set_tune(82.0);
        d.set_decay(0.5);
        for stroke in 0u8..2 {
            if stroke == 0 {
                d.dum(0.9);
            } else {
                d.tak(0.9);
            }
            let mut peak = 0.0f32;
            for _ in 0..3_000 {
                let (l, r) = d.process();
                assert!(l.is_finite() && r.is_finite(), "non-finite");
                peak = peak.max(l.abs());
            }
            assert!(peak > 0.01, "stroke {stroke} too quiet: {peak}");
        }
        // Ring out; a struck drum dies away.
        let mut peak = 0.0f32;
        d.dum(1.0);
        for i in 0..fs as usize * 2 {
            let x = d.process().0.abs();
            if i < 3_000 {
                peak = peak.max(x);
            }
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            tail = tail.max(d.process().0.abs());
        }
        assert!(tail < peak, "did not decay: tail {tail} vs peak {peak}");
        assert!(!d.active(), "daf should be silent after ringing out");
    }

    #[test]
    fn daf_jingles_add_high_frequency_energy() {
        // A struck daf with the jingles on should carry measurably more
        // high-frequency energy than the same stroke with the jingles off.
        fn high_ratio(jingle: f32) -> f32 {
            let fs = 48_000.0;
            let mut d = Daf::new(fs);
            d.set_tune(82.0);
            d.set_decay(0.5);
            d.set_jingle(jingle);
            d.tak(0.9);
            let mut prev = 0.0;
            let (mut hf, mut tot) = (0.0f32, 1e-9f32);
            for _ in 0..12_000 {
                let x = d.process().0;
                let h = x - prev;
                prev = x;
                hf += h * h;
                tot += x * x;
            }
            hf / tot
        }
        let dry = high_ratio(0.0);
        let wet = high_ratio(1.0);
        assert!(wet > dry, "jingles did not brighten: wet {wet:.4} vs dry {dry:.4}");
    }

    #[test]
    fn daf_shake_produces_sound() {
        let fs = 48_000.0;
        let mut d = Daf::new(fs);
        d.set_jingle(1.0);
        d.shake(0.9);
        let mut peak = 0.0f32;
        for _ in 0..8_000 {
            let (l, r) = d.process();
            assert!(l.is_finite() && r.is_finite(), "non-finite");
            peak = peak.max(l.abs());
        }
        assert!(peak > 0.01, "shake too quiet: {peak}");
        // A shake with the jingles muted should be (near) silent — the shake only
        // drives the jingle layer, nothing reaches the membrane path.
        let mut q = Daf::new(fs);
        q.set_jingle(0.0);
        q.shake(0.9);
        let mut peak_dry = 0.0f32;
        for _ in 0..8_000 {
            peak_dry = peak_dry.max(q.process().0.abs());
        }
        assert!(peak_dry < peak, "muted shake {peak_dry:.4} not quieter than jingled {peak:.4}");
    }
}
