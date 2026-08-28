//! Shared stereo **room / plate reverb** — a trimmed Freeverb so *every* voice
//! (not just the handpan's built-in "Air") can sit in a space. Parallel damped
//! combs feed a short series of allpasses, per channel, with the right channel
//! offset by the classic stereo spread for a decorrelated tail.
//!
//! The tail is stable by construction: every comb's feedback is derived from a
//! target decay time and hard-clamped below unity, and the recursive state is
//! flushed of denormals each sample, so there is no runaway and no denormal
//! stall. Delay lines are fixed-size, allocated once at [`Reverb::new`]
//! (firmware-friendly — no per-sample heap).
//!
//! ```text
//!            ┌─ comb×8 ─┐
//!   in ──────┤  (sum)   ├── allpass×4 ── wet_L ─┐
//!            └──────────┘                        ├─ mix ─▶ out
//!            ┌─ comb×8 ─┐  (right lines +spread) │
//!   in ──────┤  (sum)   ├── allpass×4 ── wet_R ─┘
//!            └──────────┘
//! ```
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

extern crate alloc;
use alloc::{vec, vec::Vec};

use crate::mathf;

/// Fixed input scaling into the comb bank (keeps the summed 8-comb output from
/// clipping); the classic Freeverb value.
const FIXED_GAIN: f32 = 0.015;
/// Allpass diffusion coefficient.
const AP_FEEDBACK: f32 = 0.5;
/// Fraction of `set_damp` mapped into the comb loop's one-pole cutoff.
const DAMP_SCALE: f32 = 0.4;

/// Flush denormals / vanishingly small values to zero. Denormal floats stall
/// the FPU on some targets; a long, near-silent reverb tail is exactly where
/// they accumulate, so the recursive state is scrubbed every sample.
#[inline]
fn flush(x: f32) -> f32 {
    if x.abs() < 1.0e-18 {
        0.0
    } else {
        x
    }
}

/// A damped feedback comb: a delay line whose feedback path runs through a
/// one-pole lowpass (the "damping" that rolls the tail's highs off over time).
struct Comb {
    buf: Vec<f32>,
    idx: usize,
    store: f32,
    feedback: f32,
    damp1: f32,
    damp2: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            store: 0.0,
            feedback: 0.0,
            damp1: 0.0,
            damp2: 1.0,
        }
    }

    #[inline]
    fn set_damp(&mut self, damp1: f32) {
        self.damp1 = damp1;
        self.damp2 = 1.0 - damp1;
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let out = self.buf[self.idx];
        // One-pole lowpass in the feedback loop, then flush denormals.
        self.store = flush(out * self.damp2 + self.store * self.damp1);
        self.buf[self.idx] = input + self.store * self.feedback;
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

/// A Schroeder allpass — flat magnitude, dense phase; the diffusion stage that
/// smears the comb echoes into a smooth tail.
struct Allpass {
    buf: Vec<f32>,
    idx: usize,
    feedback: f32,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], idx: 0, feedback: AP_FEEDBACK }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let buf = self.buf[self.idx];
        let out = -input + buf;
        self.buf[self.idx] = flush(input + buf * self.feedback);
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

/// A tunable stereo room/plate reverb.
///
/// # Controls
/// * [`set_decay`](Reverb::set_decay) — tail length as a target RT60 in seconds
///   (per-comb feedback is derived from each line's length so the decay is
///   roughly uniform regardless of comb tuning).
/// * [`set_size`](Reverb::set_size) — the same tail as a normalized `0..1`
///   "room size" macro (small room → long hall).
/// * [`set_damp`](Reverb::set_damp) — high-frequency damping of the tail, `0..1`.
/// * [`set_mix`](Reverb::set_mix) — dry/wet balance, `0..1` (0 = dry, 1 = wet).
/// * [`set_width`](Reverb::set_width) — stereo width of the wet tail, `0..1`
///   (1 = fully decorrelated L/R, 0 = mono).
///
/// # Processing
/// [`process`](Reverb::process) takes a stereo `(l, r)` pair and returns the
/// **mixed** `(l, r)` (dry blended with the wet tail per [`set_mix`]). Feed a
/// mono source in as `(x, x)`.
pub struct Reverb {
    combs_l: Vec<Comb>,
    combs_r: Vec<Comb>,
    aps_l: Vec<Allpass>,
    aps_r: Vec<Allpass>,
    fs: f32,
    rt60: f32,
    damp: f32,
    dry: f32,
    wet: f32,
    width: f32,
    wet1: f32,
    wet2: f32,
}

impl Reverb {
    /// Build a reverb for sample rate `fs`. Delay lines are sized here and never
    /// reallocated. Defaults: ~1.8 s decay, moderate damping, 25% wet, full width.
    pub fn new(fs: f32) -> Self {
        // Freeverb line tunings in samples @ 44.1 kHz, scaled to `fs`. The right
        // channel is offset by `spread` for a decorrelated stereo image.
        let s = fs / 44_100.0;
        let sc = |n: usize| ((n as f32 * s) as usize).max(1);
        let spread = sc(23);
        let comb_t = [1116usize, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        let ap_t = [556usize, 441, 341, 225];

        let combs_l = comb_t.iter().map(|&n| Comb::new(sc(n))).collect();
        let combs_r = comb_t.iter().map(|&n| Comb::new(sc(n) + spread)).collect();
        let aps_l = ap_t.iter().map(|&n| Allpass::new(sc(n))).collect();
        let aps_r = ap_t.iter().map(|&n| Allpass::new(sc(n) + spread)).collect();

        let mut rv = Self {
            combs_l,
            combs_r,
            aps_l,
            aps_r,
            fs,
            rt60: 1.8,
            damp: 0.25,
            dry: 0.75,
            wet: 0.25,
            width: 1.0,
            wet1: 0.0,
            wet2: 0.0,
        };
        rv.recompute_feedback();
        rv.recompute_damp();
        rv.recompute_wet();
        rv
    }

    /// Set the tail length as a target RT60 (−60 dB decay) in seconds. Clamped
    /// to a sane, always-stable range.
    pub fn set_decay(&mut self, seconds: f32) {
        self.rt60 = seconds.clamp(0.05, 20.0);
        self.recompute_feedback();
    }

    /// Room-size macro in `0..1`, mapped to RT60 (~0.3 s → ~8 s).
    pub fn set_size(&mut self, size: f32) {
        let sz = size.clamp(0.0, 1.0);
        self.set_decay(0.3 + sz * 7.7);
    }

    /// Current target decay time (RT60) in seconds.
    pub fn decay(&self) -> f32 {
        self.rt60
    }

    /// High-frequency damping of the tail, `0..1` (0 = bright, 1 = dark).
    pub fn set_damp(&mut self, damp: f32) {
        self.damp = damp.clamp(0.0, 1.0);
        self.recompute_damp();
    }

    /// Dry/wet balance, `0..1` (0 = fully dry, 1 = fully wet).
    pub fn set_mix(&mut self, mix: f32) {
        let m = mix.clamp(0.0, 1.0);
        self.dry = 1.0 - m;
        self.wet = m;
        self.recompute_wet();
    }

    /// Stereo width of the wet tail, `0..1` (1 = fully decorrelated, 0 = mono).
    pub fn set_width(&mut self, width: f32) {
        self.width = width.clamp(0.0, 1.0);
        self.recompute_wet();
    }

    /// Clear all delay lines (kill the tail without reallocating).
    pub fn reset(&mut self) {
        for c in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            for x in c.buf.iter_mut() {
                *x = 0.0;
            }
            c.store = 0.0;
            c.idx = 0;
        }
        for a in self.aps_l.iter_mut().chain(self.aps_r.iter_mut()) {
            for x in a.buf.iter_mut() {
                *x = 0.0;
            }
            a.idx = 0;
        }
    }

    /// Per-comb feedback from the target RT60: for a comb of delay `τ = len/fs`,
    /// `g` such that `g^(rt60/τ) = 10⁻³`, i.e. `g = 10^(−3τ/rt60)`. Deriving it
    /// per line makes the decay roughly uniform across the (different-length)
    /// combs, and the hard clamp below unity guarantees a bounded, decaying tail.
    fn recompute_feedback(&mut self) {
        let (fs, rt60) = (self.fs, self.rt60);
        let fb = |len: usize| -> f32 {
            let tau = len as f32 / fs;
            let g = mathf::powf(10.0, -3.0 * tau / rt60);
            // Never reach or exceed 1.0 — the stability guarantee.
            g.clamp(0.0, 0.9995)
        };
        for c in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            c.feedback = fb(c.buf.len());
        }
    }

    fn recompute_damp(&mut self) {
        let d1 = self.damp * DAMP_SCALE;
        for c in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            c.set_damp(d1);
        }
    }

    fn recompute_wet(&mut self) {
        // Freeverb stereo blend: at width=1 the two wet channels are kept fully
        // separate (wet2=0); at width=0 they are summed to mono.
        self.wet1 = self.wet * (self.width * 0.5 + 0.5);
        self.wet2 = self.wet * ((1.0 - self.width) * 0.5);
    }

    /// Process one stereo sample; returns the dry/wet-mixed `(left, right)`.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let input = (l + r) * (0.5 * FIXED_GAIN);

        let mut wl = 0.0;
        let mut wr = 0.0;
        for c in &mut self.combs_l {
            wl += c.process(input);
        }
        for c in &mut self.combs_r {
            wr += c.process(input);
        }
        for a in &mut self.aps_l {
            wl = a.process(wl);
        }
        for a in &mut self.aps_r {
            wr = a.process(wr);
        }

        let out_l = l * self.dry + wl * self.wet1 + wr * self.wet2;
        let out_r = r * self.dry + wr * self.wet1 + wl * self.wet2;
        (out_l, out_r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    /// Peak-hold envelope of a channel over 10 ms blocks; returns the block
    /// peaks so a test can find where the tail crosses a threshold.
    fn block_peaks(x: &[f32], block: usize) -> Vec<f32> {
        x.chunks(block)
            .map(|c| c.iter().fold(0.0f32, |m, &v| m.max(v.abs())))
            .collect()
    }

    /// Feed a unit impulse, run for `secs`, return (left, right) tails.
    fn impulse_response(rv: &mut Reverb, secs: f32) -> (Vec<f32>, Vec<f32>) {
        let n = (secs * FS) as usize;
        let mut l = vec![0.0f32; n];
        let mut r = vec![0.0f32; n];
        for i in 0..n {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (ol, or) = rv.process(x, x);
            l[i] = ol;
            r[i] = or;
        }
        (l, r)
    }

    /// Estimate RT60-ish: time from the tail's peak block down to −60 dB.
    fn measure_t60(x: &[f32]) -> f32 {
        let block = (FS as usize) / 100; // 10 ms
        let peaks = block_peaks(x, block);
        let global = peaks.iter().fold(0.0f32, |m, &v| m.max(v)).max(1e-12);
        let thresh = global * 0.001; // -60 dB
        // last block whose peak is still above the threshold
        let last = peaks.iter().rposition(|&p| p > thresh).unwrap_or(0);
        (last as f32) * (block as f32) / FS
    }

    #[test]
    fn decays_to_silence_after_input_stops() {
        let mut rv = Reverb::new(FS);
        rv.set_decay(1.5);
        rv.set_mix(1.0); // pure wet so we measure the tail itself
        let (l, _r) = impulse_response(&mut rv, 8.0);

        // Everything stays finite.
        assert!(l.iter().all(|v| v.is_finite()));

        // The tail actually dies: the last 0.5 s is essentially silent relative
        // to the peak.
        let tail_start = l.len() - (FS as usize / 2);
        let tail_peak = l[tail_start..].iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        let global_peak = l.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!(
            tail_peak < global_peak * 1e-3,
            "tail not silent: tail_peak={tail_peak} global_peak={global_peak}"
        );

        // Measured decay is in a plausible band around the 1.5 s target.
        let t60 = measure_t60(&l);
        assert!(t60 > 0.5 && t60 < 5.0, "implausible t60 = {t60}");
    }

    #[test]
    fn longer_decay_setting_rings_longer() {
        let short_t60 = {
            let mut rv = Reverb::new(FS);
            rv.set_decay(0.8);
            rv.set_mix(1.0);
            let (l, _) = impulse_response(&mut rv, 12.0);
            measure_t60(&l)
        };
        let long_t60 = {
            let mut rv = Reverb::new(FS);
            rv.set_decay(4.0);
            rv.set_mix(1.0);
            let (l, _) = impulse_response(&mut rv, 12.0);
            measure_t60(&l)
        };
        assert!(
            long_t60 > short_t60 * 1.5,
            "decay control not monotonic: short={short_t60} long={long_t60}"
        );
    }

    #[test]
    fn stable_under_sustained_input() {
        let mut rv = Reverb::new(FS);
        rv.set_decay(6.0); // long tail, most likely to blow up if unstable
        rv.set_mix(1.0);
        // 20 s of full-scale, sign-alternating drive (worst case for feedback).
        let n = (20.0 * FS) as usize;
        let mut peak = 0.0f32;
        for i in 0..n {
            let drive = if i % 2 == 0 { 1.0 } else { -1.0 };
            let (ol, or) = rv.process(drive, drive);
            assert!(ol.is_finite() && or.is_finite(), "non-finite at sample {i}");
            peak = peak.max(ol.abs()).max(or.abs());
        }
        // Bounded output — no runaway. Wet gain sums 8 combs, so a few units of
        // headroom is expected, but nothing explosive.
        assert!(peak < 8.0, "output not bounded: peak = {peak}");
    }

    #[test]
    fn tail_is_stereo_decorrelated() {
        let mut rv = Reverb::new(FS);
        rv.set_decay(2.5);
        rv.set_mix(1.0);
        rv.set_width(1.0);
        let (l, r) = impulse_response(&mut rv, 4.0);

        // Skip the first 50 ms (initial impulse is common to both channels).
        let start = FS as usize / 20;
        let (l, r) = (&l[start..], &r[start..]);

        // Pearson correlation of L vs R over the tail.
        let n = l.len() as f32;
        let mean = |x: &[f32]| x.iter().sum::<f32>() / n;
        let (ml, mr) = (mean(l), mean(r));
        let mut cov = 0.0f32;
        let mut vl = 0.0f32;
        let mut vr = 0.0f32;
        for i in 0..l.len() {
            let a = l[i] - ml;
            let b = r[i] - mr;
            cov += a * b;
            vl += a * a;
            vr += b * b;
        }
        let corr = cov / (vl.sqrt() * vr.sqrt() + 1e-20);
        assert!(
            corr.abs() < 0.9,
            "L/R tail not decorrelated: corr = {corr}"
        );
        // And both channels are actually carrying energy.
        assert!(vl > 1e-6 && vr > 1e-6, "a channel is silent: vl={vl} vr={vr}");
    }
}
