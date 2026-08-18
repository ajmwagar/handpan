//! The generic humanized **section** — one implementation shared by every
//! family (wind/string/mallet/…). A section stacks N copies of a [`Voice`],
//! each detuned, panned, level-varied, mod-desynced and onset-staggered, so a
//! soloist blooms into a whole desk of players. Previously this ~200-line
//! pattern was copy-pasted into every core; now it is written once.

extern crate alloc;
use alloc::vec::Vec;

use crate::mathf;

/// A single playable voice the [`Ensemble`] can stack. Each family implements
/// this over its own core (`Wind`, `Bowed`, `Mallet`, …).
pub trait Voice {
    /// Per-note event (breath, bow speed/pressure, strike velocity, …). Stored
    /// for onset-staggered players, so it must be `Copy`.
    type Event: Copy;
    /// Decorrelate this copy from the others (re-seed noise, desync LFO/vibrato).
    fn humanize(&mut self, seed: u32);
    /// Start a note at `freq` with the family's event.
    fn trigger(&mut self, freq: f32, ev: Self::Event);
    /// Release / lift (a no-op for struck voices that just ring down).
    fn release(&mut self);
    /// One mono sample.
    fn process(&mut self) -> f32;
}

/// Deterministic per-player hash → [-1, 1] (stable spread, uncorrelated players).
#[inline]
fn hash(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x5F35_6495;
    x ^= x >> 15;
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

/// Equal-power pan gains for a position in [-1, 1].
#[inline]
fn pan_gains(pos: f32) -> (f32, f32) {
    let theta = (pos + 1.0) * 0.25 * core::f32::consts::PI;
    (mathf::cos(theta), mathf::sin(theta))
}

struct Player<V: Voice> {
    voice: V,
    detune_norm: f32,
    detune_ratio: f32,
    pan_norm: f32,
    pan_l: f32,
    pan_r: f32,
    gain: f32,
    /// Per-player modulation spread (e.g. vibrato-rate factor) exposed to the
    /// family for its own fan-out controls.
    mod_scale: f32,
    stagger: u32,
    pend: Option<(f32, V::Event)>,
    countdown: u32,
}

/// A section of humanized [`Voice`]s. The active count is the "how many players"
/// control; [`Ensemble::set_spread`] and [`Ensemble::set_width`] are the tuning
/// and stereo-image macros. Family-specific controls fan out via
/// [`Ensemble::for_each_voice`].
pub struct Ensemble<V: Voice> {
    players: Vec<Player<V>>,
    active: usize,
    spread_cents: f32,
}

impl<V: Voice> Ensemble<V> {
    /// Build a section of up to `max` players. `build(i)` makes player `i`'s
    /// voice (the caller supplies `Wind::new`/`Bowed::new`/…); each is then
    /// humanized. `spread_cents` is the tuning spread; `stagger_ms` the maximum
    /// onset stagger. Player 0 is the on-pitch, centred, in-sync leader.
    pub fn new(
        fs: f32,
        max: usize,
        spread_cents: f32,
        stagger_ms: f32,
        mut build: impl FnMut(usize) -> V,
    ) -> Self {
        let max = max.max(1);
        let mut players = Vec::with_capacity(max);
        for i in 0..max {
            let mut voice = build(i);
            voice.humanize(0x51ED_2A17 ^ (i as u32).wrapping_mul(0x9E37_79B9));
            let (dnorm, pan, gain, mods, stag) = if i == 0 {
                (0.0, 0.0, 1.0, 1.0, 0)
            } else {
                let d = hash(i as u32 * 4 + 1);
                let p = hash(i as u32 * 4 + 2);
                let g = 0.85 + 0.15 * hash(i as u32 * 4 + 3).abs();
                let m = 0.85 + 0.30 * ((hash(i as u32 * 4 + 3) + 1.0) * 0.5);
                let s = (0.5 * (hash(i as u32 * 4 + 4) + 1.0) * (stagger_ms * 0.001) * fs) as u32;
                (d, p, g, m, s)
            };
            let (pl, pr) = pan_gains(pan);
            players.push(Player {
                voice,
                detune_norm: dnorm,
                detune_ratio: mathf::exp2(dnorm * spread_cents / 1200.0),
                pan_norm: pan,
                pan_l: pl,
                pan_r: pr,
                gain,
                mod_scale: mods,
                stagger: stag,
                pend: None,
                countdown: 0,
            });
        }
        Ensemble { players, active: 1, spread_cents }
    }

    /// Active player count (the "chairs/desks" control). Clamped to `[1, max]`.
    pub fn set_active(&mut self, n: usize) {
        self.active = n.clamp(1, self.players.len());
    }

    /// Active player count.
    pub fn active(&self) -> usize {
        self.active
    }

    /// Maximum player count.
    pub fn max(&self) -> usize {
        self.players.len()
    }

    /// Tuning spread across the section, in cents (intimate ↔ wide).
    pub fn set_spread(&mut self, cents: f32) {
        self.spread_cents = cents.max(0.0);
        for p in &mut self.players {
            p.detune_ratio = mathf::exp2(p.detune_norm * self.spread_cents / 1200.0);
        }
    }

    /// Stereo width, 0..1 — collapse toward centre (0) or open to the full field (1).
    pub fn set_width(&mut self, width: f32) {
        let w = width.clamp(0.0, 1.0);
        for p in &mut self.players {
            let (l, r) = pan_gains(p.pan_norm * w);
            p.pan_l = l;
            p.pan_r = r;
        }
    }

    /// Trigger a note across the active section (each player detuned + staggered).
    pub fn trigger(&mut self, freq: f32, ev: V::Event) {
        for p in self.players.iter_mut().take(self.active) {
            let f = freq * p.detune_ratio;
            if p.stagger == 0 {
                p.voice.trigger(f, ev);
                p.pend = None;
                p.countdown = 0;
            } else {
                p.pend = Some((f, ev));
                p.countdown = p.stagger;
            }
        }
    }

    /// Release the active section.
    pub fn release(&mut self) {
        for p in self.players.iter_mut().take(self.active) {
            p.voice.release();
            p.pend = None;
            p.countdown = 0;
        }
    }

    /// Fan a family-specific control out to every voice, with its per-player
    /// modulation-spread factor (e.g. `|v, s| v.set_vibrato(rate * s, depth)`).
    pub fn for_each_voice(&mut self, mut f: impl FnMut(&mut V, f32)) {
        for p in &mut self.players {
            f(&mut p.voice, p.mod_scale);
        }
    }

    /// One stereo sample of the whole section.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let (mut l, mut r) = (0.0, 0.0);
        let makeup = 1.0 / mathf::sqrt(self.active as f32);
        for p in self.players.iter_mut().take(self.active) {
            if p.countdown > 0 {
                p.countdown -= 1;
                if p.countdown == 0 {
                    if let Some((f, ev)) = p.pend.take() {
                        p.voice.trigger(f, ev);
                    }
                }
            }
            let s = p.voice.process() * p.gain * makeup;
            l += s * p.pan_l;
            r += s * p.pan_r;
        }
        (l, r)
    }
}
