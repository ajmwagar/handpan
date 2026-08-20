//! # puget-macro
//!
//! The universal **Macro Engine + Expander Bus** — the keystone layer that maps
//! a small, fixed set of *universal macros* onto every Puget instrument family
//! (handpan, wind, bowed, mallet, plucked). It is written in a **CV-first,
//! modular (Eurorack / VCV Rack) idiom** — knobs, attenuverters, normalled CV,
//! slew, gates and an instrument-agnostic message bus — not host-automation
//! floats.
//!
//! The pieces:
//!
//! * [`MacroId`] — the seven universal macros. Every macro's canonical value is
//!   a normalized `f32` in `0.0..=1.0`.
//! * [`cv`] — the CV conditioning layer: V/Oct ↔ Hz (reusing
//!   [`puget_dsp::Quantizer`]), unipolar/bipolar voltage helpers, the classic
//!   **knob + attenuverted, normalled CV** [`cv::CvInput`], a [`cv::Slew`]
//!   limiter (zipper-free knob/CV moves), and a [`cv::Gate`] Schmitt trigger.
//! * [`MacroBus`] — a fixed-size, `Copy`, alloc-free `{macro → value}` message
//!   struct: **one universal expander** fills it and hands it to any family base.
//! * [`MacroEngine`] — a module's front panel: one [`cv::CvInput`] + [`cv::Slew`]
//!   per macro, turning conditioned CV into a slewed [`MacroBus`].
//! * [`MacroTarget`] — the trait each family type implements (via the adapters in
//!   this crate, calling the families' existing public setters), so a bus
//!   [`MacroTarget::apply_bus`] routes every macro to the right parameter.
//!
//! The adapters are **feature-gated per family** (`handpan`, `wind`, `bowed`,
//! `mallet`, `plucked`) so firmware for one module compiles only its own family.
//! Dependency-free (internal workspace crates + optional `libm`), `no_std`.
//!
//! ```
//! # #[cfg(feature = "wind")] {
//! use puget_macro::{MacroBus, MacroId, MacroTarget};
//! use wind_core::{WindEnsemble, WindKind};
//!
//! let mut section = WindEnsemble::new(48_000.0, WindKind::Clarinet, 8, 7.0);
//! let mut bus = MacroBus::new();
//! bus.set(MacroId::Timbre, 0.8);  // brighter
//! bus.set(MacroId::Width, 1.0);   // full stereo field
//! bus.set(MacroId::Chairs, 0.6);  // a mid-sized section
//! section.apply_bus(&bus);        // every supported macro reaches its setter
//! # }
//! ```
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod cv;

mod bus;
mod target;

pub use bus::{MacroBus, MacroEngine, MacroMessage};
pub use target::MacroTarget;

// One adapter module per family, each behind its own feature so a single-family
// firmware build never compiles the others. The impls are `MacroTarget for
// <family type>` — the trait is local, so there is no orphan-rule problem, and
// the core crates need no edits (bar one tiny fan-out setter, noted in the doc).
#[cfg(feature = "handpan")]
mod adapt_handpan;
#[cfg(feature = "wind")]
mod adapt_wind;
#[cfg(feature = "bowed")]
mod adapt_bowed;
#[cfg(feature = "mallet")]
mod adapt_mallet;
#[cfg(feature = "plucked")]
mod adapt_plucked;

/// The seven **universal macros**. Each family maps every macro onto its own
/// concrete setter(s); where a macro doesn't apply (e.g. `Chairs` on a single,
/// non-sectioned voice) the adapter reports it unsupported and the apply is a
/// documented no-op.
///
/// Semantics (the contract every adapter honors, all driven by a normalized
/// `0.0..=1.0` value):
///
/// * [`Timbre`](MacroId::Timbre) — spectral colour, dark ↔ bright / metallic.
/// * [`Dynamics`](MacroId::Dynamics) — playing intensity: breath/bow drive for
///   sustained voices, ring-openness (sustain / un-damping) for struck ones.
/// * [`Motion`](MacroId::Motion) — modulation liveliness: vibrato, tremolo,
///   breath sway, the didgeridoo wobble, nonghyeon shake.
/// * [`Articulation`](MacroId::Articulation) — contact / attack character: bow
///   position, growl, amp grind, sympathetic halo, pitch push.
/// * [`Chairs`](MacroId::Chairs) — ensemble size: soloist ↔ full section.
/// * [`Spread`](MacroId::Spread) — detune spread across the section (cents).
/// * [`Width`](MacroId::Width) — stereo image, mono ↔ full field.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MacroId {
    Timbre = 0,
    Dynamics = 1,
    Motion = 2,
    Articulation = 3,
    Chairs = 4,
    Spread = 5,
    Width = 6,
}

/// Number of universal macros — the fixed width of a [`MacroBus`].
pub const MACRO_COUNT: usize = 7;

impl MacroId {
    /// Every macro, in id order — the canonical iteration order for a bus.
    pub const ALL: [MacroId; MACRO_COUNT] = [
        MacroId::Timbre,
        MacroId::Dynamics,
        MacroId::Motion,
        MacroId::Articulation,
        MacroId::Chairs,
        MacroId::Spread,
        MacroId::Width,
    ];

    /// Dense index in `0..MACRO_COUNT` (the bus/engine slot).
    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Recover a macro from its dense index (`None` if out of range).
    #[inline]
    pub const fn from_index(i: usize) -> Option<MacroId> {
        match i {
            0 => Some(MacroId::Timbre),
            1 => Some(MacroId::Dynamics),
            2 => Some(MacroId::Motion),
            3 => Some(MacroId::Articulation),
            4 => Some(MacroId::Chairs),
            5 => Some(MacroId::Spread),
            6 => Some(MacroId::Width),
            _ => None,
        }
    }

    /// Short human/panel label.
    pub const fn name(self) -> &'static str {
        match self {
            MacroId::Timbre => "Timbre",
            MacroId::Dynamics => "Dynamics",
            MacroId::Motion => "Motion",
            MacroId::Articulation => "Articulation",
            MacroId::Chairs => "Chairs",
            MacroId::Spread => "Spread",
            MacroId::Width => "Width",
        }
    }
}

/// Clamp to the canonical macro range `[0, 1]`. Every adapter runs its incoming
/// value through this before scaling, so out-of-range CV can't misbehave.
#[inline]
pub(crate) fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn macro_ids_round_trip_through_index() {
        for id in MacroId::ALL {
            assert_eq!(MacroId::from_index(id.index()), Some(id));
        }
        assert_eq!(MacroId::from_index(MACRO_COUNT), None);
        // ALL is dense and in slot order.
        for (i, id) in MacroId::ALL.iter().enumerate() {
            assert_eq!(id.index(), i);
        }
    }
}
