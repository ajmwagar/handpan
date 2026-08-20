//! The [`MacroTarget`] trait — how a family instrument receives macros.
//!
//! Every family type (single voice and section alike) implements this via the
//! adapters in this crate, translating a normalized `0..1` macro into calls on
//! the family's *existing* public setters. A [`MacroBus`](crate::MacroBus) then
//! drives any instrument uniformly through [`MacroTarget::apply_bus`].
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use crate::{MacroBus, MacroId};

/// An instrument that can be driven by the universal macros.
///
/// Implementors map each [`MacroId`] onto their own concrete parameter(s).
/// [`apply_macro`](MacroTarget::apply_macro) does the work; [`supports`](MacroTarget::supports)
/// declares which macros are meaningful for this instrument (a single, non-
/// sectioned voice returns `false` for `Chairs`/`Spread`, etc.), and the default
/// [`apply_bus`](MacroTarget::apply_bus) routes a whole bus, applying only the
/// macros that are both present on the bus and supported by the target.
pub trait MacroTarget {
    /// Apply one macro's normalized value (`0..1`). Applying an unsupported
    /// macro is a defined no-op.
    fn apply_macro(&mut self, id: MacroId, value: f32);

    /// Whether this macro maps to a real parameter on this instrument.
    fn supports(&self, id: MacroId) -> bool;

    /// Apply every macro on the bus that is both present and supported.
    fn apply_bus(&mut self, bus: &MacroBus) {
        for id in MacroId::ALL {
            if bus.is_present(id) && self.supports(id) {
                self.apply_macro(id, bus.get(id));
            }
        }
    }
}

// ── Shared macro→unit scaling constants ──────────────────────────────────────
//
// Reference ranges the adapters scale a `0..1` macro into. Kept here (not buried
// per-file) so the whole mapping's "feel" is tunable in one place and the design
// doc's table has a single source of truth. Each item is used by a subset of the
// family adapters, so `allow(dead_code)` covers single-family firmware builds
// where the others are compiled out.

/// Detune [`Spread`](MacroId::Spread) full scale, in cents. Matches the spread
/// values the ensembles are built with (~5–12 cents of section detune).
#[allow(dead_code)]
pub(crate) const SPREAD_MAX_CENTS: f32 = 12.0;

/// [`Chairs`](MacroId::Chairs) full-scale player count. The ensembles clamp to
/// their real maximum internally, so this is an upper nominal the macro spans;
/// `value·(MAX−1)+1` gives `1..=MAX`.
#[allow(dead_code)]
pub(crate) const CHAIRS_MAX: usize = 12;

/// Map a `0..1` [`Chairs`](MacroId::Chairs) value to a player count in
/// `1..=CHAIRS_MAX` (the ensemble re-clamps to its own maximum).
#[allow(dead_code)]
#[inline]
pub(crate) fn chairs_count(value: f32) -> usize {
    let v = crate::clamp01(value);
    1 + (v * (CHAIRS_MAX as f32 - 1.0) + 0.5) as usize
}

/// A natural default modulation rate (Hz) for the [`Motion`](MacroId::Motion)
/// macro's vibrato/tremolo, when the family exposes rate+depth as one control.
#[allow(dead_code)]
pub(crate) const MOTION_RATE_HZ: f32 = 5.5;
