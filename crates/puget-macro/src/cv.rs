//! **CV conditioning** — the modular front end for the macro engine.
//!
//! This is where the "think in volts, not host floats" discipline lives. It
//! models the standard Eurorack / VCV Rack idioms explicitly:
//!
//! * **Voltage references** — the conventions the rest of the platform already
//!   uses (see the VCV bridge and firmware): audio ±5 V, unipolar modulation
//!   0–10 V, bipolar modulation ±5 V, gates ~0/10 V, **V/Oct 1 V per octave**.
//! * [`CvInput`] — the classic **knob + attenuverter × normalled CV** control
//!   conditioner. An unpatched jack is normalled to the knob (the knob is the
//!   value); patch CV and the attenuverter adds a signed amount on top.
//! * [`Slew`] — an exponential slew limiter so knob/CV moves don't zipper.
//! * [`Gate`] — a Schmitt-triggered gate/trigger input with edge detection, plus
//!   [`velocity_from_volts`] for a unipolar velocity/accent CV.
//!
//! Pitch (V/Oct) conversion and microtonal quantization reuse
//! [`puget_dsp::tuning`] rather than re-deriving the 1 V/oct law here.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use puget_dsp::mathf;

use crate::clamp01;

// ── Voltage references ───────────────────────────────────────────────────────
//
// Chosen to match the platform's existing conventions (the `cxx` bridge notes
// "VCV audio ±5 V, V/Oct 1 V/oct, 0 V = the ding") and standard modular levels.

/// Full-scale audio, ±5 V (VCV convention; the voices output ~±1 and are scaled
/// on the way out).
pub const AUDIO_PEAK_V: f32 = 5.0;

/// Unipolar modulation full scale: 0–10 V maps to a `0..1` macro value.
pub const CV_UNIPOLAR_V: f32 = 10.0;

/// Bipolar modulation full scale: ±5 V maps to a `-1..1` attenuverter/CV signal.
pub const CV_BIPOLAR_V: f32 = 5.0;

/// Gate/trigger Schmitt thresholds (volts): rises high above `GATE_HIGH_V`,
/// falls low below `GATE_LOW_V`. The hysteresis rejects noise on slow edges.
/// Sits between a logic-low residue and the ~5–10 V of a real gate.
pub const GATE_HIGH_V: f32 = 1.0;
/// See [`GATE_HIGH_V`].
pub const GATE_LOW_V: f32 = 0.4;

// ── V/Oct pitch (1 V per octave) ─────────────────────────────────────────────

/// Frequency (Hz) for a 1 V/oct pitch, where `0 V = root_hz`. Standard Eurorack
/// one-volt-per-octave. (The inverse of [`volts_from_hz`].)
#[inline]
pub fn hz_from_volts(volts: f32, root_hz: f32) -> f32 {
    root_hz * mathf::exp2(volts)
}

/// The 1 V/oct value (volts) that a frequency sits at, relative to `root_hz`.
#[inline]
pub fn volts_from_hz(hz: f32, root_hz: f32) -> f32 {
    mathf::log2(hz.max(1e-9) / root_hz.max(1e-9))
}

// ── Unipolar / bipolar helpers ───────────────────────────────────────────────

/// Condition a unipolar modulation jack (0–[`CV_UNIPOLAR_V`] V) to a `0..1`
/// macro value. Over/undervoltage is clamped.
#[inline]
pub fn unipolar_from_volts(volts: f32) -> f32 {
    clamp01(volts / CV_UNIPOLAR_V)
}

/// The inverse of [`unipolar_from_volts`]: a `0..1` value back to volts (for CV
/// thru outputs).
#[inline]
pub fn volts_from_unipolar(x: f32) -> f32 {
    clamp01(x) * CV_UNIPOLAR_V
}

/// Condition a bipolar modulation jack (±[`CV_BIPOLAR_V`] V) to a `-1..1` signal
/// (an attenuverter's CV, or an LFO). Clamped.
#[inline]
pub fn bipolar_from_volts(volts: f32) -> f32 {
    (volts / CV_BIPOLAR_V).clamp(-1.0, 1.0)
}

/// The inverse of [`bipolar_from_volts`].
#[inline]
pub fn volts_from_bipolar(x: f32) -> f32 {
    x.clamp(-1.0, 1.0) * CV_BIPOLAR_V
}

/// A unipolar velocity/accent CV (0–[`CV_UNIPOLAR_V`] V) → strike/bow velocity
/// in `0..1`.
#[inline]
pub fn velocity_from_volts(volts: f32) -> f32 {
    unipolar_from_volts(volts)
}

// ── Knob + attenuverted, normalled CV ────────────────────────────────────────

/// The standard modular control conditioner: a **knob** (the base value) plus an
/// **attenuverter** scaling a **CV input** that is **normalled to the knob**.
///
/// * With nothing patched, the CV jack is normalled to the knob, so the output
///   is exactly the knob value — the attenuverter has nothing to act on.
/// * Patch a (unipolar `0..1`, already voltage-conditioned) CV and the output is
///   `knob + attenuverter · CV`, clamped back into `0..1`.
///
/// This is the shape every macro's front-panel control takes. Bipolar sources
/// work too — condition them with [`bipolar_from_volts`] and pass the result;
/// the attenuverter still centres the swing on the knob.
///
/// *Design note (a deliberate modular choice):* the attenuverter multiplies the
/// **patched CV**, and the knob is the resting value — rather than the CV
/// *replacing* the knob. That keeps an unpatched panel dead-simple (knob = value)
/// while a patched CV modulates *around* the knob, which is how a Maths/attenuverter
/// row reads. It is intentionally distinct from the family cores' *signal*
/// normalling (e.g. `Wind::set_breath_mod`, where an unpatched jack falls back to
/// an onboard LFO); that idiom is exposed through the [`Motion`](crate::MacroId::Motion)
/// macro instead.
#[derive(Clone, Copy, Debug)]
pub struct CvInput {
    base: f32,
    atten: f32,
}

impl Default for CvInput {
    fn default() -> Self {
        // Knob at noon, attenuverter fully open (unity, non-inverting).
        CvInput { base: 0.5, atten: 1.0 }
    }
}

impl CvInput {
    /// Build with an explicit knob (`0..1`) and attenuverter (`-1..1`).
    pub fn new(knob: f32, atten: f32) -> Self {
        CvInput { base: clamp01(knob), atten: atten.clamp(-1.0, 1.0) }
    }

    /// Set the knob (base value), `0..1`.
    #[inline]
    pub fn set_knob(&mut self, knob: f32) {
        self.base = clamp01(knob);
    }

    /// Set the attenuverter, `-1..1` (negative inverts the CV).
    #[inline]
    pub fn set_atten(&mut self, atten: f32) {
        self.atten = atten.clamp(-1.0, 1.0);
    }

    /// The knob value.
    #[inline]
    pub fn knob(&self) -> f32 {
        self.base
    }

    /// The attenuverter value.
    #[inline]
    pub fn atten(&self) -> f32 {
        self.atten
    }

    /// Evaluate the conditioned control value in `0..1`.
    ///
    /// `cv` is the patched input already conditioned to a normalized signal
    /// (unipolar via [`unipolar_from_volts`], or bipolar via
    /// [`bipolar_from_volts`]); pass `None` when the jack is unpatched — the CV
    /// is then normalled to the knob and the output is simply the knob.
    #[inline]
    pub fn eval(&self, cv: Option<f32>) -> f32 {
        match cv {
            None => self.base,
            Some(v) => clamp01(self.base + self.atten * v),
        }
    }
}

// ── Slew limiter ─────────────────────────────────────────────────────────────

/// A one-pole (exponential) slew limiter. A step toward a new target settles
/// **monotonically** (no overshoot), so macro knob/CV jumps glide instead of
/// zippering. Cheap enough for per-sample use; also fine per-block.
#[derive(Clone, Copy, Debug)]
pub struct Slew {
    y: f32,
    coeff: f32,
}

impl Slew {
    /// A slew that reaches ~63% of a step in `time_ms` at sample rate `fs`.
    /// `time_ms = 0` (or a non-positive `fs`) makes it instantaneous.
    pub fn new(fs: f32, time_ms: f32) -> Self {
        let mut s = Slew { y: 0.0, coeff: 1.0 };
        s.set_time(fs, time_ms);
        s
    }

    /// Retune the glide time (e.g. from a Slew knob/CV).
    pub fn set_time(&mut self, fs: f32, time_ms: f32) {
        if time_ms <= 0.0 || fs <= 0.0 {
            self.coeff = 1.0;
            return;
        }
        let tau_samples = time_ms * 0.001 * fs;
        // One-pole coefficient: y += coeff·(target - y). Bounded to (0, 1].
        self.coeff = (1.0 - mathf::exp(-1.0 / tau_samples)).clamp(1e-6, 1.0);
    }

    /// Jump the state to `value` with no glide (e.g. on a preset recall).
    #[inline]
    pub fn reset(&mut self, value: f32) {
        self.y = value;
    }

    /// The current (smoothed) value without advancing.
    #[inline]
    pub fn value(&self) -> f32 {
        self.y
    }

    /// Advance one step toward `target`, returning the new smoothed value.
    #[inline]
    pub fn process(&mut self, target: f32) -> f32 {
        self.y += self.coeff * (target - self.y);
        self.y
    }
}

// ── Gate / trigger ───────────────────────────────────────────────────────────

/// A rising/falling/steady classification of a gate edge this sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// The gate just went high (a trigger — strike, note-on, clock).
    Rising,
    /// The gate just went low (a release / note-off).
    Falling,
    /// No transition.
    None,
}

/// A Schmitt-triggered gate input. Feed it raw volts; it reports edges (with
/// hysteresis between [`GATE_LOW_V`] and [`GATE_HIGH_V`]) and holds a high/low
/// state — the standard "did a trigger arrive this sample" input.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gate {
    high: bool,
}

impl Gate {
    /// A gate starting low.
    pub fn new() -> Self {
        Gate { high: false }
    }

    /// Whether the gate is currently high.
    #[inline]
    pub fn is_high(&self) -> bool {
        self.high
    }

    /// Process one sample of gate voltage, returning the edge (if any).
    #[inline]
    pub fn process(&mut self, volts: f32) -> Edge {
        if self.high {
            if volts < GATE_LOW_V {
                self.high = false;
                return Edge::Falling;
            }
        } else if volts > GATE_HIGH_V {
            self.high = true;
            return Edge::Rising;
        }
        Edge::None
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use puget_dsp::{Quantizer, ScalaScale};

    #[test]
    fn unipolar_and_bipolar_round_trip() {
        for &x in &[0.0f32, 0.25, 0.5, 0.9, 1.0] {
            assert!((unipolar_from_volts(volts_from_unipolar(x)) - x).abs() < 1e-6);
        }
        for &x in &[-1.0f32, -0.3, 0.0, 0.7, 1.0] {
            assert!((bipolar_from_volts(volts_from_bipolar(x)) - x).abs() < 1e-6);
        }
        // Reference points: 10 V = full unipolar, 5 V = +1 bipolar.
        assert!((unipolar_from_volts(10.0) - 1.0).abs() < 1e-6);
        assert!((bipolar_from_volts(5.0) - 1.0).abs() < 1e-6);
        // Out of range clamps.
        assert_eq!(unipolar_from_volts(20.0), 1.0);
        assert_eq!(bipolar_from_volts(-20.0), -1.0);
    }

    #[test]
    fn voct_round_trips_and_is_one_volt_per_octave() {
        let root = 261.63; // C4
        // 0 V is the root; +1 V is one octave up (×2); −1 V is an octave down.
        assert!((hz_from_volts(0.0, root) - root).abs() < 1e-3);
        assert!((hz_from_volts(1.0, root) - root * 2.0).abs() < 1e-2);
        assert!((hz_from_volts(-1.0, root) - root * 0.5).abs() < 1e-2);
        for &v in &[-2.0f32, -0.7, 0.0, 0.3, 1.5] {
            let hz = hz_from_volts(v, root);
            assert!((volts_from_hz(hz, root) - v).abs() < 1e-4, "voct not invertible at {v}");
        }
    }

    #[test]
    fn voct_quantizes_through_the_shared_quantizer() {
        // V/Oct → Hz → snap to a scale → back to volts, reusing puget_dsp.
        let root = 261.63;
        let q = Quantizer::new(ScalaScale::edo12(), root);
        // A pitch 30 cents sharp of the +7 semitone (a fifth, 0.5833 V) snaps
        // back to exactly the fifth.
        let fifth_v = 7.0 / 12.0;
        let sharp = hz_from_volts(fifth_v + 0.30 / 12.0, root);
        let snapped_hz = q.quantize_hz(sharp);
        let snapped_v = volts_from_hz(snapped_hz, root);
        assert!((snapped_v - fifth_v).abs() < 1e-3, "expected the fifth, got {snapped_v} V");
    }

    #[test]
    fn cvinput_normals_to_knob_and_attenuverts() {
        let ci = CvInput::new(0.3, 0.5);
        // Unpatched → the knob, exactly.
        assert!((ci.eval(None) - 0.3).abs() < 1e-6);
        // Patched → knob + atten·cv.
        assert!((ci.eval(Some(1.0)) - 0.8).abs() < 1e-6); // 0.3 + 0.5·1.0
        assert!((ci.eval(Some(0.4)) - 0.5).abs() < 1e-6); // 0.3 + 0.5·0.4
        // Negative attenuverter inverts.
        let inv = CvInput::new(0.6, -1.0);
        assert!((inv.eval(Some(0.4)) - 0.2).abs() < 1e-6); // 0.6 − 0.4
        // Result stays clamped to 0..1.
        assert_eq!(CvInput::new(0.9, 1.0).eval(Some(1.0)), 1.0);
        assert_eq!(CvInput::new(0.1, -1.0).eval(Some(1.0)), 0.0);
    }

    #[test]
    fn slew_step_settles_monotonically_toward_target() {
        let mut s = Slew::new(48_000.0, 5.0);
        s.reset(0.0);
        let mut prev = 0.0f32;
        let mut last = 0.0f32;
        for _ in 0..48_000 {
            let y = s.process(1.0);
            assert!(y >= prev - 1e-7, "slew overshot/reversed: {prev} -> {y}");
            assert!(y <= 1.0 + 1e-6, "slew exceeded target");
            prev = y;
            last = y;
        }
        assert!(last > 0.999, "slew did not reach target: {last}");

        // Zero glide time is instantaneous.
        let mut inst = Slew::new(48_000.0, 0.0);
        inst.reset(0.0);
        assert!((inst.process(0.7) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn gate_detects_edges_with_hysteresis() {
        let mut g = Gate::new();
        assert_eq!(g.process(0.0), Edge::None);
        // Rise through the high threshold.
        assert_eq!(g.process(5.0), Edge::Rising);
        assert!(g.is_high());
        // A dip that stays above the low threshold does NOT retrigger.
        assert_eq!(g.process(0.6), Edge::None);
        assert!(g.is_high());
        // Fall below the low threshold.
        assert_eq!(g.process(0.1), Edge::Falling);
        assert!(!g.is_high());
        // Re-arm and rise again.
        assert_eq!(g.process(10.0), Edge::Rising);
    }
}
