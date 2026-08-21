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
    /// Lip reed + long drone tube, shaped by a swept vocal-tract formant — the
    /// "wah/wobble" of a didgeridoo. Circular-breathing drone; fixed low pitch.
    Didgeridoo,
    /// Persian end-blown reed flute (**ney**): an air-jet edge tone on an open
    /// bore — the flute's jet drive, but voiced far breathier and more
    /// hollow/throaty, with a prominent breath-noise onset and clean register
    /// overblowing. Intimate and vocal, clearly distinct from the concert flute.
    Ney,
    /// Iranian double-reed shawm (**sorna**, zurna family): the single-reed
    /// conical loop driven by a steeper reed table, radiating strong even
    /// harmonics through a nasal formant — very loud, bright, buzzy and nasal.
    /// The outdoor partner to the dohol.
    Sorna,
    /// Irish **tin whistle** (penny whistle): a small fipple/whistle flute — the
    /// flute's air-jet oscillator byte-for-byte (proven register stability), but
    /// voiced bright, pure and simple, with a clean two-register overblow and a
    /// penny-whistle "chiff" on the attack. Shriller and cleaner up top than the
    /// concert flute; not breathy/hollow like the ney.
    TinWhistle,
    /// Irish **wooden transverse flute**: the flute's air-jet oscillator, voiced
    /// woodier, breathier and rounder than the silver concert flute — a warm,
    /// reedy-but-airy folk tone with a soft top and a prominent breath layer.
    IrishFlute,
    /// **Uilleann pipes** — the bellows-blown Irish bagpipe. Modeled as the
    /// **chanter**: the sax/sorna single-reed conical loop (proven stable),
    /// voiced bright, sweet and nasal with strong even harmonics (all-harmonic).
    /// Carries an internal **drone** bank (the pipes' constant tonic drones,
    /// tonic in octaves) sounded via [`Wind::set_drone`]; the chordal regulators
    /// are out of scope.
    UilleannPipes,
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

    // Dynamic reed (spike): the reed tip modeled as a one-DOF damped mass-spring
    // resonator with state (y, y_dot) instead of the static memoryless table.
    // `y` is the tip opening (rest = reed_y0, closes/beats against the lay at 0);
    // resonance `reed_w0` (rad/s) sits a couple kHz up; `reed_g` = 2·zeta·w0 is
    // the damping; `reed_drive` scales the pressure-difference force; `reed_wflow`
    // scales the quasi-static Bernoulli volume flow u = w·y·sign(dP)·sqrt(|dP|).
    // Gated by `dyn_reed`; when false the classic static path runs unchanged.
    dyn_reed: bool,
    reed_y: f32,
    reed_yd: f32,
    reed_w0: f32,
    reed_g: f32,
    reed_y0: f32,
    reed_comp: f32,
    reed_wflow: f32,
    // Output normalization for the dynamic reed: the loop is run well above the
    // oscillation threshold (for a monotone louder-with-more-breath response),
    // which makes its internal amplitude hot; this scales the *output copy* back
    // to a static-comparable level without touching the loop feedback.
    reed_norm: f32,

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

    // Didgeridoo: a swept vocal-tract formant — an SVF band-pass whose centre a
    // low LFO sweeps for the "wah" wobble, plus a manual centre (from Timbre).
    svf_lp: f32,
    svf_bp: f32,
    formant_phase: f32,
    wobble_rate: f32,
    wobble_depth: f32,
    formant_bias: f32,

    // Breath pressure, slewed so attacks/releases aren't clicks.
    breath_target: f32,
    breath_env: f32,
    attack_rate: f32,
    release_rate: f32,
    // Onset "chiff": a short breathy transient at note-on (naturalness cue).
    onset: u32,
    onset_len: f32,

    // Breath noise + vibrato.
    rng: u32,
    noise_gain: f32,
    noise_lp: f32,
    // Extra poles that shape the raw white noise into a band-limited "air"
    // spectrum (a second low-pass to kill the raspy top, a slow low-pass whose
    // subtraction removes the rumble) instead of broadband hiss.
    noise_lp2: f32,
    noise_hp: f32,
    // Last sample's injected aperture flow — the turbulence is scaled by it so
    // the breath noise pulses *with* the tone (real turbulence is made by the
    // flow) rather than sitting on top as a steady hiss.
    flow: f32,
    // Slow zero-mean random walk driving a few-cent pitch flutter — a real
    // player's air is never dead-steady, and that micro-instability is a big part
    // of why a fixed-pitch model reads as synthetic.
    pitch_walk: f32,
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
    // Sax even generator: the squared even term is fed from a fundamental-biased
    // (low-passed) copy of the loop output, so it radiates a clean 2nd harmonic
    // (h1²) instead of a mush of h1·h3 / h3² cross-products. The fixed ~800 Hz
    // cutoff makes low notes fuller (more sub-cutoff evens) and high notes purer
    // — the real conical-bore/tonehole trend. `even_lp_c` is the one-pole coeff.
    even_lp: f32,
    even_lp_c: f32,
    // Sax body resonance: a fixed low-mid formant (~330 Hz) giving the tone a
    // woodwind body rather than a bare filtered reed. Applied only for the sax.
    body: Peaking,
    // Sax "bloom": a level/harmonic envelope that opens the note over ~150 ms
    // after the chiff (0 → 1). Starts the note slightly darker/softer.
    bloom: f32,
    bloom_c: f32,
    // Sax growl (expression, off by default): a slow sub-audio amplitude flutter.
    growl_depth: f32,
    growl_rate: f32,
    growl_phase: f32,
    dc_x1: f32,
    dc_y1: f32,

    // Output radiation low-pass (two cascaded one-poles = 12 dB/oct): the steep
    // upper-harmonic roll-off a real bore/bell has. `out_pole` closer to 1 = darker.
    out_pole: f32,
    out_lp: f32,
    out_lp2: f32,

    // Uilleann drone bank: the pipes' constant tonic drones (tonic in octaves),
    // independent of the chanter loop — cheap lowpassed-sawtooth reed oscillators
    // summed under the chanter output. Stable by construction (no feedback), so
    // they never destabilize the reed. `set_drone` populates the three octaves.
    drone_on: bool,
    drone_freqs: [f32; 3],
    drone_phase: [f32; 3],
    drone_lp: [f32; 3],
    drone_lp_c: f32,
    drone_gain: [f32; 3],

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
            // Didgeridoo: driven by the stable reed loop (fundamental-strong,
            // no mode-hop) rather than a mode-hopping lip — the deep drone comes
            // out reliably, then the vocal formant makes it a didgeridoo.
            WindKind::Didgeridoo => (0.7, -0.44, 1.4, 0.05),
            // Ney: no reed table (jet drive, like the flute). Much stronger
            // breath noise than the flute (0.14) — the ney's defining airy,
            // throaty breath layer that reads as an intimate reed-flute.
            WindKind::Ney => (0.0, 0.0, 1.0, 0.16),
            // Sorna: single-reed loop, low/focused breath noise so the buzz stays
            // bright. Output gain trimmed (1.9 → 1.0) for the dynamic reed's higher
            // crest factor — the beating tip plus the boosted even term make sharp
            // peaks, so the nominal gain is lowered to keep the full-breath peak
            // under ~1.0 across the register (no clipping pre-normalization).
            WindKind::Sorna => (0.7, -0.50, 1.0, 0.035),
            // Tin whistle: no reed table (jet drive, like the flute). Low breath
            // noise — the penny whistle is pure and clean, not airy like the ney
            // (0.16) or the concert flute (0.14).
            WindKind::TinWhistle => (0.0, 0.0, 1.0, 0.06),
            // Irish flute: no reed table (jet drive). Breathier than the silver
            // concert flute (0.14) — the wooden folk flute's warm air layer.
            WindKind::IrishFlute => (0.0, 0.0, 1.0, 0.17),
            // Uilleann chanter: the sax/sorna single-reed loop (offset 0.7,
            // slope -0.50 — the PROVEN-stable reed; a steeper table choked the
            // sorna oscillator, so the chanter reuses the shallow one). Moderate
            // output; low breath noise so the reed reads sweet and focused.
            WindKind::UilleannPipes => (0.7, -0.50, 1.6, 0.04),
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
            // fundamental-strong (no octave overblow). Darker loop damp (0.57)
            // than a bright reed so the odd h3/h5 leakage drops and the radiated
            // 2nd harmonic can dominate, as on a real alto.
            WindKind::Saxophone => (0.95, 0.57),
            // Didgeridoo: the clarinet reed loop (fundamental-strong). Very high
            // loop gain so the very low drone self-sustains robustly (the reed
            // clips and self-limits, so it can't run away); bright/buzzy so the
            // drone is rich; the vocal formant voices it.
            WindKind::Didgeridoo => (0.9999, 0.45),
            // Trumpet: loss = low-freq bell reflection gain; damp = the bell
            // one-pole low-pass coeff (~1.5 kHz cutoff at bright 0.5). Loss was
            // 0.99, which made the bore modes so high-Q that the anti-resonances
            // between them cut razor-deep notches — an f0/brightness-dependent
            // comb null that gutted h3 (a 20+ dB hole reading hollow/muted).
            // Broadening the modes to loss 0.93 fills those troughs, so the mid
            // (h3) stays full and flat-topped across the register and the whole
            // brightness swing, while still self-oscillating with margin.
            WindKind::Trumpet => (0.93, 0.82),
            // Ney: the flute's inverting jet loop, byte-for-byte in the
            // oscillator (loss/damp, jet ratios, tune_scale, breath window) so it
            // inherits the flute's proven register stability — a self-oscillating
            // jet is bistable, and even a small extra loop damping tipped it into
            // dropping the octave. The ney's hollow, breathy, throaty character
            // comes entirely from the non-feedback stages: heavy breath noise, a
            // vocal radiation formant, a softer top, and a long breath onset.
            WindKind::Ney => (0.95, 0.60),
            // Sorna: the sax's inverting single-reed loop, but brighter (damp
            // 0.50 vs 0.57) so the odd stack stays strong and buzzy under the
            // radiated evens — the piercing double-reed shawm color.
            WindKind::Sorna => (0.95, 0.55),
            // Tin whistle: the flute's inverting jet loop, byte-for-byte in the
            // oscillator (loss/damp identical) so it inherits the flute's proven
            // register stability. Brightness/purity come from the non-feedback
            // stages, exactly as the ney's breathiness does.
            WindKind::TinWhistle => (0.95, 0.60),
            // Irish flute: the flute's jet loop, byte-for-byte (identical
            // oscillator → identical proven pitch/register stability). Its
            // woodier, rounder tone comes from the radiation stages, not the loop.
            WindKind::IrishFlute => (0.95, 0.60),
            // Uilleann chanter: the sax's inverting single-reed loop, bright
            // (damp 0.55, like the sorna) so the odd stack stays present under
            // the radiated evens — a sweet, nasal, all-harmonic reed.
            WindKind::UilleannPipes => (0.95, 0.55),
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
            // Saxophone: a formant pair that replaces the old monotonic roll-off
            // with the measured alto scatter — an h3 notch (~760 Hz cut, so the
            // 3rd/4th of low notes dip like the real horn) and the reedy "honk"
            // resurge lifted at ~1.25 kHz (fills h4/h5 of low notes, h3 of the
            // upper register). Post-loop EQ only — the reed loop is untouched.
            WindKind::Saxophone => {
                [Peaking::new(760.0, 1.6, -6.5, fs), Peaking::new(1250.0, 1.0, 9.0, fs)]
            }
            // Trumpet: the brass formant ("bridge") sits ~1.2–1.5 kHz — that is
            // where the real Iowa trumpet peaks (its h4), not up at 2.4 kHz.
            WindKind::Trumpet => {
                [Peaking::new(1200.0, 1.0, 11.0, fs), Peaking::new(4500.0, 0.7, -4.0, fs)]
            }
            // Didgeridoo: the swept vocal formant carries the timbre; here just a
            // low-body lift and a soft high cut.
            WindKind::Didgeridoo => {
                [Peaking::new(120.0, 0.6, 4.0, fs), Peaking::new(4000.0, 0.7, -4.0, fs)]
            }
            // Ney: a throaty vocal formant (~850 Hz lift) for the hollow,
            // breathy body, plus a soft high cut — airy and intimate rather
            // than the flute's brighter, purer edge tone.
            WindKind::Ney => {
                [Peaking::new(850.0, 1.0, 5.0, fs), Peaking::new(3200.0, 0.8, -5.0, fs)]
            }
            // Sorna: the nasal shawm "bark" — a strong midrange formant (~1.5 kHz)
            // and an upper-presence lift (~2.8 kHz) that give the double reed its
            // piercing, buzzy, nasal projection.
            WindKind::Sorna => {
                [Peaking::new(1500.0, 1.4, 10.0, fs), Peaking::new(2800.0, 1.1, 6.0, fs)]
            }
            // Tin whistle: bright and a bit shrill — a mid presence lift plus a
            // strong upper-presence lift (~3 kHz) for the penny whistle's clean,
            // piercing top (the opposite of the flute's soft high cut).
            WindKind::TinWhistle => {
                [Peaking::new(800.0, 0.7, 2.0, fs), Peaking::new(3000.0, 1.0, 6.0, fs)]
            }
            // Irish flute: woody and round — a warm low-body lift and a deeper
            // high cut than the silver concert flute (-8 vs -6 dB), so the top is
            // soft and the tone reads wooden and reedy-but-airy.
            WindKind::IrishFlute => {
                [Peaking::new(450.0, 0.8, 4.0, fs), Peaking::new(3000.0, 0.7, -8.0, fs)]
            }
            // Uilleann chanter: the nasal reed "cry" — a strong mid formant
            // (~1.4 kHz) and an upper-presence lift (~2.4 kHz) for the sweet,
            // nasal, projecting chanter voice (softer/sweeter than the sorna's
            // harder shawm bark).
            WindKind::UilleannPipes => {
                [Peaking::new(1400.0, 1.3, 8.0, fs), Peaking::new(2400.0, 1.0, 4.0, fs)]
            }
        };
        // Flute jet parameters (jet_ratio, jet_refl, end_refl, tune_scale,
        // tune_off) and its breath window (bias, scale). tune_scale corrects the
        // jet-drive pitch (empirical for jet_ratio 0.30 + damp 0.60).
        let (jet_ratio, jet_refl, end_refl, tune_scale, tune_off) = match kind {
            WindKind::Flute => (0.30, 0.5, 0.5, 1.542, 0.0),
            // Ney: the same jet embouchure and tuning as the flute (identical
            // oscillator core → identical, proven pitch and register stability).
            WindKind::Ney => (0.30, 0.5, 0.5, 1.542, 0.0),
            // Tin whistle & Irish flute: the flute's jet embouchure and tuning
            // byte-for-byte — the whole fipple/transverse-flute family shares the
            // proven-stable jet oscillator; only the radiation voicing differs.
            WindKind::TinWhistle => (0.30, 0.5, 0.5, 1.542, 0.0),
            WindKind::IrishFlute => (0.30, 0.5, 0.5, 1.542, 0.0),
            _ => (0.0, 0.0, 0.0, 1.0, 0.0),
        };
        let (flute_breath_bias, flute_breath_scale) = (0.87, 0.07);
        // Dynamic-reed parameters (spike). `reed_hz` = the tip's mechanical
        // resonance (a couple kHz — well above the played note, so it colours the
        // upper harmonics without tracking pitch); `reed_zeta` = its damping
        // ratio; `reed_comp` = opening-excursion scale (a fraction of the static
        // reed table's slope — 0.5 is a shallow valve that stays open under a firm
        // blow, so loudness rises with breath instead of choking); `reed_wflow` =
        // the Bernoulli flow scale, set well above the oscillation threshold for a
        // robust breath response; `reed_norm` scales the (hot) output copy back to
        // a static-comparable RMS. Only the single-reed voices use these.
        let (reed_hz, reed_zeta, reed_comp, reed_wflow, reed_norm) = match kind {
            WindKind::Clarinet => (2600.0f32, 0.40f32, 0.50f32, 1.60f32, 0.22f32),
            // Sorna: a small, stiff double reed — higher resonance, less damped
            // so it buzzes brightly.
            WindKind::Sorna => (3200.0, 0.32, 0.50, 1.40, 0.22),
            WindKind::Saxophone => (2200.0, 0.42, 0.50, 1.50, 0.26),
            WindKind::Didgeridoo => (1600.0, 0.55, 0.50, 1.50, 0.30),
            WindKind::UilleannPipes => (2800.0, 0.38, 0.50, 1.50, 0.26),
            _ => (2500.0, 0.45, 0.50, 1.50, 0.28),
        };
        let reed_w0 = core::f32::consts::TAU * reed_hz;
        let reed_g = 2.0 * reed_zeta * reed_w0;
        Wind {
            fs,
            kind,
            bore: Delay::new(max),
            refl: Reflect::new(refl_loss, refl_damp),
            radiate,
            freq: 220.0,
            reed_offset,
            reed_slope,
            // The sorna ships on the dynamic reed by default (validated: brighter,
            // buzzier, and stable — see the spike report). Every other voice stays
            // on the static path until individually validated.
            dyn_reed: matches!(kind, WindKind::Sorna),
            reed_y: 0.7,
            reed_yd: 0.0,
            reed_w0,
            reed_g,
            reed_y0: 0.7,
            reed_comp,
            reed_wflow,
            reed_norm,
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
            svf_lp: 0.0,
            svf_bp: 0.0,
            formant_phase: 0.0,
            wobble_rate: 4.5,
            wobble_depth: 1.0,
            formant_bias: 0.5,
            breath_target: 0.0,
            breath_env: 0.0,
            attack_rate: 0.0,
            release_rate: 0.0,
            onset: 0,
            onset_len: 1.0,
            rng: 0x2545_F491 ^ (kind as u32).wrapping_mul(0x9E37_79B9),
            noise_gain,
            noise_lp: 0.0,
            noise_lp2: 0.0,
            noise_hp: 0.0,
            flow: 0.0,
            pitch_walk: 0.0,
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
            even_lp: 0.0,
            // ~800 Hz one-pole: passes the fundamental strongly, tapers the
            // upper harmonics out of the squared even source.
            even_lp_c: 1.0 - mathf::exp(-core::f32::consts::TAU * 800.0 / fs),
            // Body formant: a low-mid resonance for woodwind warmth (sax only;
            // transparent 0 dB for the other voices so the slot is a no-op).
            body: match kind {
                WindKind::Saxophone => Peaking::new(330.0, 0.9, 5.0, fs),
                _ => Peaking::new(330.0, 1.0, 0.0, fs),
            },
            bloom: 1.0,
            // ~150 ms opening time constant.
            bloom_c: 1.0 - mathf::exp(-core::f32::consts::TAU * 1.1 / fs),
            growl_depth: 0.0,
            growl_rate: 5.5,
            growl_phase: 0.0,
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
                    WindKind::Didgeridoo => 3500.0,
                    // Ney: soften the top so the tone stays hollow/intimate; the
                    // breath noise still carries the air up top.
                    WindKind::Ney => 6500.0,
                    // Sorna: a high cutoff — the shawm's buzz and nasal upper
                    // harmonics must project, so keep the top open.
                    WindKind::Sorna => 5000.0,
                    // Tin whistle: very open top — brighter/shriller than the
                    // concert flute so the penny whistle's clean edge cuts through.
                    WindKind::TinWhistle => 12_000.0,
                    // Irish flute: darker top than the concert flute (9 kHz) for
                    // the wooden, rounded folk tone; the breath carries the air.
                    WindKind::IrishFlute => 6000.0,
                    // Uilleann chanter: open enough for the reedy, nasal buzz to
                    // project, but a touch sweeter than the sorna's 5 kHz.
                    WindKind::UilleannPipes => 4500.0,
                };
                mathf::exp(-core::f32::consts::TAU * fc / fs)
            },
            out_lp: 0.0,
            out_lp2: 0.0,
            drone_on: false,
            drone_freqs: [0.0; 3],
            drone_phase: [0.0, 0.33, 0.66],
            drone_lp: [0.0; 3],
            // ~1.2 kHz one-pole: softens the top of the reed-pipe drone so the
            // bed is warm and hollow, never harsh.
            drone_lp_c: 1.0 - mathf::exp(-core::f32::consts::TAU * 1200.0 / fs),
            // Tenor loudest, the lower octaves progressively softer.
            drone_gain: [0.30, 0.24, 0.18],
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
            // Didgeridoo drone: quarter-wave lip-tube (a low, lossy pipe).
            WindKind::Didgeridoo => self.fs / self.freq * 0.5 - 1.0 - fgd,
            // Ney: open-open jet bore, exactly like the flute (its own tune_scale).
            WindKind::Ney => self.fs / self.freq * self.tune_scale - self.tune_off - 1.0 - fgd,
            // Sorna: conical bore on the quarter-wave single-reed loop (like the
            // sax); the even harmonics are radiated at the output, not by the bore.
            WindKind::Sorna => self.fs / self.freq * 0.5 - 1.0 - fgd,
            // Tin whistle & Irish flute: open-open jet bore, exactly like the
            // flute (their own tune_scale — identical to the flute's).
            WindKind::TinWhistle | WindKind::IrishFlute => {
                self.fs / self.freq * self.tune_scale - self.tune_off - 1.0 - fgd
            }
            // Uilleann chanter: conical bore on the quarter-wave single-reed loop
            // (like the sax/sorna); evens radiated at the output, not by the bore.
            WindKind::UilleannPipes => self.fs / self.freq * 0.5 - 1.0 - fgd,
        };
        let d = d.max(4.0);
        self.bore_delay = d;
        self.bore.set_delay(d);
        if matches!(
            self.kind,
            WindKind::Flute | WindKind::Ney | WindKind::TinWhistle | WindKind::IrishFlute
        ) {
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
        // A breathy reed "chiff" at the attack — the naturalness cue the reeds
        // were missing. Longer/airier on the sax, brief on the clarinet.
        let ms = match self.kind {
            WindKind::Saxophone => 0.06,
            WindKind::Clarinet => 0.025,
            // Ney: a long, prominent breath-noise onset — the airy "haa" of an
            // end-blown flute catching the edge. The defining attack of the voice.
            WindKind::Ney => 0.09,
            // Sorna: a short, hard double-reed chiff.
            WindKind::Sorna => 0.035,
            // Tin whistle: the crisp penny-whistle "chiff" — a short, bright
            // breath spit as the fipple catches at the note-on. The signature
            // articulation of the whistle.
            WindKind::TinWhistle => 0.03,
            // Irish flute: a breathier, slightly longer wooden-flute onset.
            WindKind::IrishFlute => 0.05,
            // Uilleann chanter: a short reed chiff as the reed speaks.
            WindKind::UilleannPipes => 0.03,
            _ => 0.0,
        };
        self.onset_len = (ms * self.fs).max(1.0);
        self.onset = self.onset_len as u32;
        // Sax bloom starts closed and opens over the first ~150 ms.
        if self.kind == WindKind::Saxophone {
            self.bloom = 0.0;
        }
        // Dynamic reed: start the tip from rest so a re-trigger can't carry stale
        // velocity into the new note (a click / brief blowup).
        if self.dyn_reed {
            self.reed_y = self.reed_y0;
            self.reed_yd = 0.0;
        }
    }

    /// Sax growl (expression): a gentle sub-audio amplitude flutter. `depth`
    /// 0 = off (default); ~0.05–0.2 is a subtle-to-throaty growl. `rate_hz` is
    /// the flutter speed (~5–8 Hz is a natural growl).
    pub fn set_growl(&mut self, depth: f32, rate_hz: f32) {
        self.growl_depth = depth.clamp(0.0, 0.5);
        self.growl_rate = rate_hz.max(0.0);
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
            // Compressed sax breath map. The reed's stable window closes off
            // above a target of ~0.64 (offset 0.7, slope -0.50): the old
            // 0.32 + 0.34·b reached ~0.66 at full breath and choked the reed
            // (silent notes at breath ≥ ~0.94). Topping at ~0.58 keeps full
            // breath the loudest point while staying inside the stable window
            // across G3–D5 and all brightness settings.
            WindKind::Saxophone => 0.30 + 0.28 * b,
            WindKind::Trumpet => 0.30 + 0.45 * b,
            // Circular-breathing drone: steady pressure in the reed's sweet spot
            // — enough to keep the low reed oscillating, not so much it chokes.
            WindKind::Didgeridoo => 0.60 + 0.16 * b,
            // Ney: the flute's exact jet blowing window — a self-oscillating jet
            // is register-bistable, so the ney reuses the flute's proven-stable
            // window verbatim. Breathiness comes from the heavy breath noise, not
            // from a wider/lower (unstable) window.
            WindKind::Ney => self.flute_breath_bias + self.flute_breath_scale * b,
            // Sorna: blown hard, but the steeper reed's stable window closes off
            // sooner than the sax's — top out at ~0.55 so full breath stays the
            // loudest point while the reed keeps oscillating across the register.
            WindKind::Sorna => 0.30 + 0.28 * b,
            // Tin whistle & Irish flute: the flute's exact narrow jet blowing
            // window (onset ~0.87, overblows above ~1.0) — the jet is
            // register-bistable, so the whole flute family reuses the proven
            // window verbatim. The tin whistle's two-register overblow is the
            // flute's own octave jump, reached by blowing harder within this map.
            WindKind::TinWhistle | WindKind::IrishFlute => {
                self.flute_breath_bias + self.flute_breath_scale * b
            }
            // Uilleann chanter: the sax/sorna reed window (tops out at ~0.58,
            // inside the shallow reed's stable range across the chanter register).
            WindKind::UilleannPipes => 0.30 + 0.28 * b,
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
        // On the didgeridoo, "vibrato" is the wobble: the rate and depth of the
        // vocal-formant sweep (the wah rhythm), not a pitch vibrato.
        if self.kind == WindKind::Didgeridoo {
            self.wobble_rate = rate_hz.max(0.0);
            self.wobble_depth = (depth * 2.0).clamp(0.0, 1.0);
            return;
        }
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
            // Reedy but kept darker than before so h2 stays dominant over the
            // odd stack; the swing still opens toward a buzzier embouchure.
            WindKind::Saxophone => 0.66 - 0.18 * a,
            // Moves the bell cutoff (brighter = higher cutoff = smaller g).
            WindKind::Trumpet => 0.87 - 0.10 * a,
            WindKind::Didgeridoo => 0.55,
            // Ney: the flute's exact narrow damp swing — keeps the jet oscillator
            // identical to the flute's (proven register stability).
            WindKind::Ney => 0.64 - 0.08 * a,
            // Sorna: the dynamic reed already generates a rich, buzzy harmonic
            // stack on its own, so its brightness knob sweeps a NARROWER, DARKER
            // loop-damp range than the static reed's — keeping the loop dark lets
            // the reed's inherent brightness read as body across 0..1 instead of
            // going harsh/thin at the top. (The static path keeps the old range.)
            WindKind::Sorna if self.dyn_reed => 0.74 - 0.12 * a,
            WindKind::Sorna => 0.63 - 0.16 * a,
            // Tin whistle & Irish flute: the flute's exact narrow damp swing —
            // keeps the jet oscillator identical to the flute's (proven register
            // stability); a wider swing destabilizes the jet.
            WindKind::TinWhistle | WindKind::IrishFlute => 0.64 - 0.08 * a,
            // Uilleann chanter: opens toward a bright, nasal reed like the sorna.
            WindKind::UilleannPipes => 0.63 - 0.16 * a,
        };
        // Timbre also biases the didgeridoo's vocal-formant centre (mouth shape).
        if self.kind == WindKind::Didgeridoo {
            self.formant_bias = a;
        }
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

    /// Dynamic reed (spike): advance the one-DOF tip resonator one sample and
    /// return the quasi-static Bernoulli volume flow injected into the bore.
    ///
    /// The tip is a damped mass-spring with state `(reed_y, reed_yd)`. `reed_y`
    /// is the opening. Its equilibrium tracks the same operating point the proven
    /// static valve settles on — `y_target = clamp(offset + slope·pdiff)`, with
    /// `pdiff = p_bore − p_mouth` — so the reed sits open under a steady blow
    /// instead of choking shut, but the mass-spring gives the tip real inertia:
    /// it cannot follow `y_target` instantly, lags it, and rings near its ~kHz
    /// resonance (`reed_w0`), which is the dynamic colour the static curve lacks.
    /// `reed_comp` scales how hard the pressure swing drives the tip (beating
    /// depth). The opening is floored at 0 where the reed beats shut against the
    /// lay. Flow follows Bernoulli through the opening,
    /// `u = wflow·y·sign(dP)·sqrt(|dP|)` with `dP = p_mouth − p_bore = −pdiff`.
    /// Symplectic (semi-implicit) Euler keeps the audio-rate resonator stable.
    #[inline]
    fn dyn_reed_flow(&mut self, refl: f32, breath: f32) -> f32 {
        let pdiff = refl - breath;
        let dt = 1.0 / self.fs;
        // Target opening = the static reed table's operating point (keeps the
        // reed open under a steady blow — the DC balance the raw physical sign
        // got wrong). `reed_comp` scales the pressure-driven excursion.
        let mut y_target = self.reed_offset + self.reed_comp * self.reed_slope * pdiff;
        if y_target > 1.0 {
            y_target = 1.0;
        }
        // Damped mass-spring pulling the tip toward the target: inertia + a ~kHz
        // resonance the memoryless table cannot produce.
        let accel =
            self.reed_w0 * self.reed_w0 * (y_target - self.reed_y) - self.reed_g * self.reed_yd;
        self.reed_yd += accel * dt;
        self.reed_y += self.reed_yd * dt;
        // Beating/collision clamp: the reed cannot pass through the lay.
        if self.reed_y < 0.0 {
            self.reed_y = 0.0;
            if self.reed_yd < 0.0 {
                self.reed_yd = 0.0;
            }
        }
        // Quasi-static Bernoulli volume flow through the opening. dP = −pdiff.
        // The sqrt law is regularized as dP/sqrt(|dP|+eps): still ~sign·sqrt(|dP|)
        // for a firm blow, but with a finite (not infinite) slope through dP=0, so
        // the oscillation onset is a soft threshold instead of a hard jump.
        let dp = -pdiff;
        self.reed_wflow * self.reed_y * dp / mathf::sqrt(dp.abs() + 0.12)
    }

    /// Enable/disable the dynamic reed (spike A/B toggle). When off (default) the
    /// classic static reed-table path runs unchanged. Resets the reed state so
    /// the note starts from rest.
    pub fn set_dynamic_reed(&mut self, on: bool) {
        self.dyn_reed = on;
        self.reed_y = self.reed_y0;
        self.reed_yd = 0.0;
    }

    /// Override the dynamic-reed parameters (spike tuning/A-B probe). `hz` = tip
    /// resonance, `zeta` = damping ratio, `comp` = static compliance (opening
    /// closes by comp·dP), `wflow` = Bernoulli flow scale.
    pub fn set_reed_params(&mut self, hz: f32, zeta: f32, comp: f32, wflow: f32) {
        self.reed_w0 = core::f32::consts::TAU * hz;
        self.reed_g = 2.0 * zeta * self.reed_w0;
        self.reed_comp = comp;
        self.reed_wflow = wflow;
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

        // Sax bloom: rises 0 → 1 over ~150 ms after the attack — the note opens
        // up (level + even-harmonic body) once the chiff settles.
        self.bloom += self.bloom_c * (1.0 - self.bloom);

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

        // Turbulence noise, shaped like real breath rather than broadband hiss.
        // Two cascaded one-poles roll the raspy top off at 12 dB/oct (~1.2 kHz),
        // and subtracting a slow low-pass removes the sub-band rumble — leaving a
        // band-limited "air" spectrum.
        let n = self.white();
        self.noise_lp += 0.2 * (n - self.noise_lp);
        // The air-jet voices (flute family) are legitimately breathy and their
        // *bistable* jet relies on the full-spectrum air to hold its register, so
        // they keep the original broadband breath noise. The reeds and brass —
        // where the raspy buzz actually lives — get a de-rasped turbulence:
        // band-limited (raspy top rolled off), flow-correlated (pulsing with the
        // tone instead of hissing over it), and a touch lower in level.
        let is_jet = matches!(
            self.kind,
            WindKind::Flute | WindKind::Ney | WindKind::TinWhistle | WindKind::IrishFlute
        );
        let noise = if is_jet {
            self.noise_lp
        } else {
            self.noise_lp2 += 0.18 * (self.noise_lp - self.noise_lp2);
            self.noise_hp += 0.02 * (self.noise_lp2 - self.noise_hp);
            let air = (self.noise_lp2 - self.noise_hp) * 4.0;
            // Turbulence generated by the flow through the aperture (a floor
            // keeps a little air at the flow nodes).
            let turb = 0.5 + 0.5 * (self.flow.abs() * 3.0).min(1.0);
            air * turb
        };
        let mut breath = self.breath_env * self.breath_move;
        breath += breath * self.noise_gain * noise;
        // Attack chiff: a decaying breath-noise burst over the onset window —
        // the airy "tah" a reed makes as it catches. Fades to the steady tone.
        if self.onset > 0 {
            self.onset -= 1;
            let env = self.onset as f32 / self.onset_len; // 1 → 0
            breath += self.breath_env * (0.5 * env * env) * self.noise_lp;
        }
        if self.vib_depth > 0.0 {
            self.vib_phase += core::f32::consts::TAU * self.vib_rate / self.fs;
            if self.vib_phase > core::f32::consts::TAU {
                self.vib_phase -= core::f32::consts::TAU;
            }
            breath += breath * self.vib_depth * 0.1 * mathf::sin(self.vib_phase);
        }

        // A few-cent pitch flutter: a slow zero-mean random walk on the bore
        // length, so the tone breathes like real air instead of sitting on a
        // dead-steady pitch. Peaks ~±4 cents; it averages to the true pitch, so
        // tuning is unaffected. Set here so this sample's bore read uses it (the
        // trumpet re-applies it with its own wave-steepening delay modulation).
        self.pitch_walk += 0.0003 * (self.white() - self.pitch_walk);
        let eff_delay = self.bore_delay * (1.0 - self.pitch_walk * 0.06);
        self.bore.set_delay(eff_delay);

        let mut out = match self.kind {
            // Single reed (clarinet cylindrical / sax conical): the reflected
            // bore pressure drives the nonlinear reed, scattered back in. The
            // clarinet's loop inverts (odd harmonics); the sax's does not (all).
            WindKind::Clarinet
            | WindKind::Saxophone
            | WindKind::Didgeridoo
            | WindKind::Sorna
            | WindKind::UilleannPipes => {
                let refl = self.refl.tick(self.bore.last_out());
                if self.dyn_reed {
                    // Dynamic reed: the tip resonator sets the opening; a
                    // quasi-static Bernoulli flow is injected in place of the table.
                    let inj = self.dyn_reed_flow(refl, breath);
                    self.flow = inj;
                    // The Bernoulli flow falls as bore pressure rises, so it feeds
                    // the reflected wave back with the opposite sign of the static
                    // table; subtracting restores the single loop inversion (the
                    // quarter-wave fundamental) instead of jumping to the octave.
                    // The bore keeps the full (hot) loop amplitude; only the output
                    // copy is scaled to a static-comparable level.
                    self.bore.tick(breath - inj) * self.reed_norm
                } else {
                    let pdiff = refl - breath;
                    let inj = pdiff * self.reed(pdiff);
                    self.flow = inj; // aperture flow → drives the turbulence
                    self.bore.tick(breath + inj)
                }
            }
            // Air jet (flute): the reflected bore pressure (DC-blocked) drives a
            // cubic jet nonlinearity through the embouchure delay, summed with
            // the end reflection back into the open bore. All harmonics.
            WindKind::Flute | WindKind::Ney | WindKind::TinWhistle | WindKind::IrishFlute => {
                let filt = self.refl.tick(self.bore.last_out());
                let temp = filt - self.jdc_x1 + 0.995 * self.jdc_y1;
                self.jdc_x1 = filt;
                self.jdc_y1 = temp;
                let mut pd = breath - self.jet_refl * temp;
                pd = self.jet.tick(pd);
                let jet = (pd * (pd * pd - 1.0)).clamp(-1.0, 1.0);
                self.flow = jet; // jet flow → drives the turbulence
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
                self.flow = area * delta; // lip flow → drives the turbulence
                let inj = reflected + area * delta;
                // Brassiness = wave steepening: high-pressure fronts travel
                // faster → the bore delay shortens with pressure. DC-blocked so
                // the mean delay (pitch) is fixed while harmonics brighten.
                let bl = (0.6 + 0.4 * self.bright) * self.breath_env;
                self.brass_dc += 0.001 * (bore_out - self.brass_dc);
                let dmod = (self.brass * bl * (bore_out - self.brass_dc)).clamp(-3.0, 3.0);
                self.bore.set_delay(eff_delay - dmod);
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
            WindKind::Saxophone => 2.6,
            WindKind::Didgeridoo => 0.5, // fill the buzzy all-harmonic drone
            // Sorna: strong evens turn the odd-only reed loop into a full,
            // all-harmonic double reed. Uses the raw squared term (not the sax's
            // fundamental-biased copy) so the upper cross-products add buzz. The
            // dynamic reed runs at a lower (normalized) output level, so its
            // squared term needs a larger coefficient to radiate the same 2nd
            // harmonic — the buzzy "all-harmonic" balance the static reed had.
            WindKind::Sorna if self.dyn_reed => 6.0,
            WindKind::Sorna => 2.0,
            // Uilleann chanter: strong evens turn the odd-only reed loop into the
            // sweet, nasal, all-harmonic chanter — a touch less than the sorna so
            // it reads sweeter than the harder shawm.
            WindKind::UilleannPipes => 1.8,
            _ => 0.0,
        };
        if even_amt > 0.0 {
            // Sax: square a fundamental-biased (low-passed) copy so the even
            // term is a clean 2nd harmonic (h1²) rather than a fizzy mush of
            // h1·h3 / h3² cross-products; the bloom opens it over the onset.
            let (src, amt) = if matches!(self.kind, WindKind::Saxophone) {
                self.even_lp += self.even_lp_c * (out - self.even_lp);
                (self.even_lp, even_amt * (0.55 + 0.45 * self.bloom))
            } else {
                (out, even_amt)
            };
            let sq = src * src;
            self.even_dc += 0.0008 * (sq - self.even_dc);
            out += amt * (sq - self.even_dc);
        }

        // Sax body resonance: a fixed low-mid formant for woodwind warmth.
        if matches!(self.kind, WindKind::Saxophone) {
            out = self.body.tick(out);
            // Growl (expression): a gentle sub-audio amplitude flutter.
            if self.growl_depth > 0.0 {
                self.growl_phase += core::f32::consts::TAU * self.growl_rate / self.fs;
                if self.growl_phase > core::f32::consts::TAU {
                    self.growl_phase -= core::f32::consts::TAU;
                }
                out *= 1.0 + self.growl_depth * mathf::sin(self.growl_phase);
            }
        }

        // Didgeridoo vocal-tract formant: a resonant band-pass (state-variable)
        // whose centre a low LFO sweeps — the "wah" wobble that is the whole
        // character of the instrument. Timbre biases the centre (mouth shape).
        if self.kind == WindKind::Didgeridoo {
            self.formant_phase += core::f32::consts::TAU * self.wobble_rate / self.fs;
            if self.formant_phase > core::f32::consts::TAU {
                self.formant_phase -= core::f32::consts::TAU;
            }
            let sweep = 0.5 * (1.0 - mathf::cos(self.formant_phase)); // 0..1
            // Centre glides ~350 Hz → ~1.8 kHz, biased by Timbre.
            let center = 350.0
                + (0.25 + 0.55 * self.formant_bias) * 1500.0
                + self.wobble_depth * sweep * 1100.0;
            let f = (2.0 * mathf::sin(core::f32::consts::PI * center / self.fs)).min(1.4);
            let damp = 0.22; // resonance (lower = more vocal/peaky)
            self.svf_lp += f * self.svf_bp;
            let hp = out - self.svf_lp - damp * self.svf_bp;
            self.svf_bp += f * hp;
            // Drone body + strong vocal band → the didgeridoo voice.
            out = 0.45 * out + 1.7 * self.svf_bp;
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
        // Sax level bloom: the note swells slightly as it opens (subtle).
        let bloom_gain = if matches!(self.kind, WindKind::Saxophone) {
            0.9 + 0.1 * self.bloom
        } else {
            1.0
        };
        let mut y_out = self.out_lp2 * self.out_gain * bloom_gain;

        // Uilleann drones: the pipes' constant tonic drones, tonic in octaves.
        // A pipe drone is a reed on a cylindrical pipe — a hollow, mellow,
        // ODD-harmonic tone (like a clarinet), not the brassy all-harmonic buzz
        // of a raw sawtooth. Synthesize it additively from a few odd partials
        // (no aliasing, no harsh edge), and let a per-sample one-pole soften the
        // top a touch further. The three octaves are detuned a hair (in
        // `set_drone`) so they beat slowly instead of locking into a static buzz.
        if self.drone_on {
            let mut d = 0.0;
            for i in 0..3 {
                if self.drone_freqs[i] <= 0.0 {
                    continue;
                }
                self.drone_phase[i] += self.drone_freqs[i] / self.fs;
                if self.drone_phase[i] >= 1.0 {
                    self.drone_phase[i] -= 1.0;
                }
                let ph = core::f32::consts::TAU * self.drone_phase[i];
                // Hollow reed-pipe tone: odd harmonics with a gentle rolloff.
                let reed = mathf::sin(ph)
                    + 0.30 * mathf::sin(3.0 * ph)
                    + 0.12 * mathf::sin(5.0 * ph)
                    + 0.05 * mathf::sin(7.0 * ph);
                self.drone_lp[i] += self.drone_lp_c * (reed - self.drone_lp[i]);
                d += self.drone_gain[i] * self.drone_lp[i];
            }
            y_out += d;
        }
        y_out
    }

    /// Sound (or silence) the uilleann pipes' constant **drones** — the tonic
    /// held underneath the chanter melody. `hz` is the tenor-drone pitch (the
    /// tonic); the bank also sounds it one and two octaves below (baritone and
    /// bass), the classic uilleann drone stack. `on` starts/stops the whole bank.
    /// The drones are independent low-frequency oscillators (no reed feedback),
    /// so they are stable by construction and keep sounding between chanter notes.
    /// A no-op on the other voices. The chordal regulators are out of scope.
    pub fn set_drone(&mut self, hz: f32, on: bool) {
        self.drone_on = on && self.kind == WindKind::UilleannPipes;
        if hz > 0.0 {
            // Tenor (tonic), baritone (−1 oct), bass (−2 oct), the lower octaves
            // detuned a few cents so their upper partials beat slowly against the
            // tonic — a living drone bed rather than a static, phase-locked buzz.
            self.drone_freqs = [hz, hz * 0.5 * 1.0015, hz * 0.25 * 0.9990];
        }
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
    /// Sax growl (expression, off by default), section-wide.
    pub fn set_growl(&mut self, depth: f32, rate_hz: f32) {
        self.section.for_each_voice(|v, _| v.set_growl(depth, rate_hz));
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
    fn didgeridoo_drones_and_stops() {
        let fs = 48_000.0;
        let mut v = Wind::new(fs, WindKind::Didgeridoo);
        v.set_vibrato(4.5, 0.5);
        v.note_on(73.42, 0.9); // D2 drone
        // Must self-sustain a low drone (not decay away).
        let mut early = 0.0f32;
        let mut late = 0.0f32;
        for i in 0..fs as usize * 2 {
            let s = v.process();
            assert!(s.is_finite(), "non-finite");
            if (fs as usize..fs as usize + 4_000).contains(&i) {
                early = early.max(s.abs());
            }
            if i >= fs as usize * 2 - 4_000 {
                late = late.max(s.abs());
            }
        }
        assert!(late > 0.05, "didge did not sustain a drone: {late}");
        assert!(late > early * 0.3, "didge decayed too much: {early} -> {late}");
        // In tune (autocorrelation).
        let f = measured_hz(&mut v, fs, 4_000, 16_384);
        let cents = 1200.0 * (f / 73.42).log2();
        assert!(cents.abs() < 30.0, "didge off by {cents:.1} cents ({f:.1})");
        v.note_off();
        for _ in 0..fs as usize {
            v.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            tail = tail.max(v.process().abs());
        }
        assert!(tail < late, "didge did not stop after note_off");
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
    fn sax_sounds_at_full_breath() {
        // Regression: the saxophone used to go SILENT at breath ≥ ~0.94 — the
        // breath map drove the reed target past its stable window, choking the
        // oscillation (measured RMS ~0.00 at breath 0.95–1.0). Every earlier
        // test blew at ≤ 0.9, so the dead-zone slipped through. Assert audible,
        // in-tune output at FULL breath across the whole G3–D5 register.
        let fs = 48_000.0;
        for &(f0, name) in &[
            (196.0f32, "G3"),
            (261.63, "C4"),
            (329.63, "E4"),
            (392.0, "G4"),
            (523.25, "C5"),
            (587.33, "D5"),
        ] {
            let mut v = Wind::new(fs, WindKind::Saxophone);
            v.note_on(f0, 1.0); // full breath
            for _ in 0..24_000 {
                v.process();
            }
            let buf: Vec<f32> = (0..16_384).map(|_| v.process()).collect();
            let rms =
                (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt();
            assert!(rms.is_finite(), "sax {name} non-finite at full breath");
            assert!(rms > 0.1, "sax {name} silent at full breath: rms {rms:.4}");
            // Must still self-oscillate IN TUNE at full breath.
            let f = measured_hz(&mut v, fs, 4_000, 16_384);
            let cents = 1200.0 * (f / f0).log2();
            assert!(
                cents.abs() < 30.0,
                "sax {name} out of tune at full breath: {cents:.1} cents ({f:.1} Hz)"
            );
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

    #[test]
    fn ney_and_sorna_sound_and_stop() {
        // Both new Persian voices must self-oscillate cleanly (finite, audible,
        // no runaway) and die away after note_off — mirroring the reed voices.
        let fs = 48_000.0;
        // Ney blown in its stable range (C4/E4/G4); sorna across its register.
        for &(kind, f0) in &[
            (WindKind::Ney, 293.66f32),   // D4
            (WindKind::Ney, 392.0),       // G4
            (WindKind::Sorna, 196.0),     // G3
            (WindKind::Sorna, 293.66),    // D4
        ] {
            let mut v = Wind::new(fs, kind);
            v.note_on(f0, 0.9);
            let mut peak = 0.0f32;
            for i in 0..fs as usize {
                let s = v.process();
                assert!(s.is_finite(), "{kind:?} non-finite");
                if i > fs as usize / 2 {
                    peak = peak.max(s.abs());
                }
            }
            assert!(peak > 0.01, "{kind:?} @ {f0} did not sound: {peak}");
            assert!(peak < 20.0, "{kind:?} @ {f0} runaway: {peak}");
            v.note_off();
            for _ in 0..fs as usize {
                v.process();
            }
            let mut tail = 0.0f32;
            for _ in 0..4_800 {
                tail = tail.max(v.process().abs());
            }
            assert!(tail < peak, "{kind:?} @ {f0} did not stop");
        }
    }

    #[test]
    fn ney_and_sorna_in_tune() {
        // Pitch accuracy across a couple of pitches each, like the sax/trumpet
        // tuning test. The ney is a jet edge tone: it holds its register cleanly
        // across C4 and up (its musical range); the sorna is a single-reed loop
        // stable across its whole register.
        let fs = 48_000.0;
        // (kind, f0, breath) — the ney is voiced for C4-up, blown near full air.
        let cases: &[(WindKind, f32, f32)] = &[
            (WindKind::Ney, 261.63, 0.9),   // C4
            (WindKind::Ney, 349.23, 0.9),   // F4
            (WindKind::Ney, 440.0, 0.9),    // A4
            (WindKind::Ney, 523.25, 0.9),   // C5
            (WindKind::Sorna, 146.83, 0.85), // D3
            (WindKind::Sorna, 196.0, 0.85),  // G3
            (WindKind::Sorna, 293.66, 0.85), // D4
            (WindKind::Sorna, 392.0, 0.85),  // G4
        ];
        for &(kind, f0, breath) in cases {
            let mut v = Wind::new(fs, kind);
            v.set_brightness(0.5);
            v.note_on(f0, breath);
            let f = measured_hz(&mut v, fs, 24_000, 16_384);
            let cents = 1200.0 * (f / f0).log2();
            assert!(cents.abs() < 25.0, "{kind:?} {f0}Hz off by {cents:.1} cents ({f:.1})");
        }
    }

    #[test]
    fn ney_and_sorna_audible_across_breath() {
        // Both voices must produce audible output across the usable breath range,
        // not only at one pressure. The ney (jet) speaks even at soft air; the
        // sorna (reed) needs a minimum pressure to start, so its range begins a
        // little above zero — sweep from there up.
        let fs = 48_000.0;
        let ney_breaths = [0.2f32, 0.4, 0.6, 0.8, 1.0];
        let sorna_breaths = [0.35f32, 0.55, 0.75, 1.0];
        for &(kind, f0, breaths) in &[
            (WindKind::Ney, 392.0f32, &ney_breaths[..]),
            (WindKind::Sorna, 261.63, &sorna_breaths[..]),
        ] {
            for &breath in breaths {
                let mut v = Wind::new(fs, kind);
                v.set_brightness(0.5);
                v.note_on(f0, breath);
                for _ in 0..24_000 {
                    v.process();
                }
                let buf: Vec<f32> = (0..16_384).map(|_| v.process()).collect();
                let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt();
                assert!(rms.is_finite(), "{kind:?} non-finite at breath {breath}");
                assert!(
                    rms > 0.05,
                    "{kind:?} inaudible at breath {breath}: rms {rms:.4}"
                );
            }
        }
    }

    #[test]
    fn irish_winds_sound_and_stop() {
        // The three Irish voices must self-oscillate cleanly (finite, audible, no
        // runaway) and die after note_off. The two jet voices (tin whistle, Irish
        // flute) are blown in the flute family's stable D4-up register; the reed
        // chanter is stable across its whole range.
        let fs = 48_000.0;
        for &(kind, f0) in &[
            (WindKind::TinWhistle, 293.66f32), // D4
            (WindKind::TinWhistle, 523.25),    // C5
            (WindKind::IrishFlute, 293.66),    // D4
            (WindKind::IrishFlute, 440.0),     // A4
            (WindKind::UilleannPipes, 196.0),  // G3
            (WindKind::UilleannPipes, 293.66), // D4
        ] {
            let mut v = Wind::new(fs, kind);
            v.note_on(f0, 0.9);
            let mut peak = 0.0f32;
            for i in 0..fs as usize {
                let s = v.process();
                assert!(s.is_finite(), "{kind:?} non-finite");
                if i > fs as usize / 2 {
                    peak = peak.max(s.abs());
                }
            }
            assert!(peak > 0.01, "{kind:?} @ {f0} did not sound: {peak}");
            assert!(peak < 20.0, "{kind:?} @ {f0} runaway: {peak}");
            v.note_off();
            for _ in 0..fs as usize {
                v.process();
            }
            let mut tail = 0.0f32;
            for _ in 0..4_800 {
                tail = tail.max(v.process().abs());
            }
            assert!(tail < peak, "{kind:?} @ {f0} did not stop");
        }
    }

    #[test]
    fn irish_winds_in_tune() {
        // Pitch accuracy across a few pitches each. The two jet voices hold their
        // register cleanly from D4 up (the flute family's stable range, and the
        // real whistle/Irish-flute range); the reed chanter is in tune across its
        // whole register.
        let fs = 48_000.0;
        let cases: &[(WindKind, f32, f32)] = &[
            (WindKind::TinWhistle, 293.66, 0.9), // D4
            (WindKind::TinWhistle, 392.0, 0.9),  // G4
            (WindKind::TinWhistle, 440.0, 0.9),  // A4
            (WindKind::TinWhistle, 587.33, 0.9), // D5
            (WindKind::IrishFlute, 293.66, 0.9), // D4
            (WindKind::IrishFlute, 392.0, 0.9),  // G4
            (WindKind::IrishFlute, 523.25, 0.9), // C5
            (WindKind::UilleannPipes, 196.0, 0.85), // G3
            (WindKind::UilleannPipes, 293.66, 0.85), // D4
            (WindKind::UilleannPipes, 392.0, 0.85),  // G4
            (WindKind::UilleannPipes, 587.33, 0.85), // D5
        ];
        for &(kind, f0, breath) in cases {
            let mut v = Wind::new(fs, kind);
            v.set_brightness(0.5);
            v.note_on(f0, breath);
            let f = measured_hz(&mut v, fs, 24_000, 16_384);
            let cents = 1200.0 * (f / f0).log2();
            assert!(cents.abs() < 25.0, "{kind:?} {f0}Hz off by {cents:.1} cents ({f:.1})");
        }
    }

    #[test]
    fn irish_winds_audible_across_breath() {
        // Each voice must be audible across the usable breath range. The jet
        // voices are swept on G4 (in-register at every breath); the reed chanter
        // from a low pressure up.
        let fs = 48_000.0;
        let jet_breaths = [0.2f32, 0.4, 0.6, 0.8, 1.0];
        let reed_breaths = [0.25f32, 0.5, 0.75, 1.0];
        for &(kind, f0, breaths) in &[
            (WindKind::TinWhistle, 392.0f32, &jet_breaths[..]),
            (WindKind::IrishFlute, 392.0, &jet_breaths[..]),
            (WindKind::UilleannPipes, 293.66, &reed_breaths[..]),
        ] {
            for &breath in breaths {
                let mut v = Wind::new(fs, kind);
                v.set_brightness(0.5);
                v.note_on(f0, breath);
                for _ in 0..24_000 {
                    v.process();
                }
                let buf: Vec<f32> = (0..16_384).map(|_| v.process()).collect();
                let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt();
                assert!(rms.is_finite(), "{kind:?} non-finite at breath {breath}");
                assert!(rms > 0.05, "{kind:?} inaudible at breath {breath}: rms {rms:.4}");
            }
        }
    }

    #[test]
    fn tin_whistle_brighter_than_irish_flute() {
        // Character contrast: the tin whistle is shrill/pure up top; the wooden
        // Irish flute is round and dark. At the same note the whistle carries far
        // more upper-harmonic energy relative to its fundamental.
        let fs = 48_000.0;
        let f0 = 587.33f32; // D5
        let hi_ratio = |kind| -> f32 {
            let mut v = Wind::new(fs, kind);
            v.set_brightness(0.6);
            v.note_on(f0, 0.9);
            for _ in 0..24_000 {
                v.process();
            }
            let buf: Vec<f32> = (0..16_384).map(|_| v.process()).collect();
            let h1 = harmonic(&buf, f0, 1.0, fs).max(1e-9);
            let hi = harmonic(&buf, f0, 4.0, fs) + harmonic(&buf, f0, 5.0, fs);
            hi / h1
        };
        let whistle = hi_ratio(WindKind::TinWhistle);
        let flute = hi_ratio(WindKind::IrishFlute);
        assert!(
            whistle > flute,
            "tin whistle should be brighter than the Irish flute: whistle {whistle:.4} vs flute {flute:.4}"
        );
    }

    #[test]
    fn uilleann_chanter_all_harmonic() {
        // The chanter must radiate strong even harmonics (like the sax/sorna) — a
        // full, nasal, all-harmonic reed rather than the clarinet's odd-only tone.
        let fs = 48_000.0;
        let f0 = 293.66f32; // D4
        let mut v = Wind::new(fs, WindKind::UilleannPipes);
        v.set_brightness(0.5);
        v.note_on(f0, 0.9);
        for _ in 0..24_000 {
            v.process();
        }
        let buf: Vec<f32> = (0..16_384).map(|_| v.process()).collect();
        let h1 = harmonic(&buf, f0, 1.0, fs);
        let h2 = harmonic(&buf, f0, 2.0, fs);
        let h3 = harmonic(&buf, f0, 3.0, fs);
        let strongest = h1.max(h3).max(1e-9);
        assert!(
            h2 > 0.1 * strongest,
            "uilleann chanter missing even harmonics: h1={h1:.3} h2={h2:.3} h3={h3:.3}"
        );
    }

    #[test]
    fn uilleann_drone_sounds_and_holds() {
        // The drones must sound a steady tonic bed with no chanter note playing,
        // stay finite, and sustain (they are constant, unlike the chanter). And
        // set_drone must be a no-op on the other voices.
        let fs = 48_000.0;
        let mut v = Wind::new(fs, WindKind::UilleannPipes);
        v.set_drone(146.83, true); // D3 tonic drone stack
        // No chanter note — only the drones sound.
        let mut early = 0.0f32;
        let mut late = 0.0f32;
        for i in 0..fs as usize * 2 {
            let s = v.process();
            assert!(s.is_finite(), "drone non-finite");
            if (fs as usize / 2..fs as usize / 2 + 4_000).contains(&i) {
                early = early.max(s.abs());
            }
            if i >= fs as usize * 2 - 4_000 {
                late = late.max(s.abs());
            }
        }
        assert!(late > 0.05, "drones did not sound: {late}");
        assert!(late > early * 0.5, "drones decayed (should be constant): {early} -> {late}");
        // Tuned to the tonic: the drone bed spans four octaves (tenor + two lower
        // octaves), so its composite period is too long for the autocorrelation
        // window — check the tonic pitch spectrally instead. The tonic (146.83)
        // and its octaves must dominate a nearby off-pitch probe (190 Hz).
        for _ in 0..4_000 {
            v.process();
        }
        let buf: Vec<f32> = (0..16_384).map(|_| v.process()).collect();
        let tonic = harmonic(&buf, 146.83, 1.0, fs) + harmonic(&buf, 73.42, 1.0, fs);
        let off = harmonic(&buf, 190.0, 1.0, fs);
        assert!(tonic > 3.0 * off.max(1e-9), "drone not on the tonic: tonic {tonic:.3} off {off:.3}");
        // Turning the drones off silences the bed.
        v.set_drone(0.0, false);
        for _ in 0..fs as usize {
            v.process();
        }
        let mut off = 0.0f32;
        for _ in 0..4_800 {
            off = off.max(v.process().abs());
        }
        assert!(off < late, "drones did not stop when turned off");

        // set_drone is a no-op on non-uilleann voices: a silent clarinet stays
        // silent even with the drone "on".
        let mut c = Wind::new(fs, WindKind::Clarinet);
        c.set_drone(200.0, true);
        for _ in 0..24_000 {
            c.process();
        }
        let buf: Vec<f32> = (0..16_384).map(|_| c.process()).collect();
        let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt();
        assert!(rms < 0.01, "set_drone leaked onto the clarinet: rms {rms:.4}");
    }

    #[test]
    fn sorna_is_bright_and_all_harmonic() {
        // The double-reed shawm must radiate strong even harmonics (like the sax)
        // — a buzzy, all-harmonic tone, not the clarinet's odd-only hollow one.
        let fs = 48_000.0;
        let f0 = 261.63f32;
        let mut v = Wind::new(fs, WindKind::Sorna);
        v.set_brightness(0.6);
        v.note_on(f0, 0.9);
        for _ in 0..24_000 {
            v.process();
        }
        let buf: Vec<f32> = (0..16_384).map(|_| v.process()).collect();
        let h1 = harmonic(&buf, f0, 1.0, fs);
        let h2 = harmonic(&buf, f0, 2.0, fs);
        let h3 = harmonic(&buf, f0, 3.0, fs);
        let strongest = h1.max(h3).max(1e-9);
        assert!(
            h2 > 0.1 * strongest,
            "sorna missing even harmonics: h1={h1:.3} h2={h2:.3} h3={h3:.3}"
        );
    }
}
