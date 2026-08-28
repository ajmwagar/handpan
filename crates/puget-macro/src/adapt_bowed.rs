//! Bowed family adapter: maps the universal macros onto [`Bowed`] (a single
//! bowed string) and [`BowedEnsemble`] (a string section).
//!
//! A bowed string is self-sustained by stick-slip friction, so Dynamics is the
//! bow itself. On the single voice, harder Dynamics drives the bow faster *and*
//! heavier (a natural coupling), and Articulation sweeps the bow position from
//! sul tasto to sul ponticello. The section's per-note bow energy is set at
//! note-on (via the trigger event, not a live setter), so the [`BowedEnsemble`]
//! leaves Dynamics unsupported.
//!
//! | Macro | Bowed parameter |
//! |---|---|
//! | Timbre | `set_brightness`, `0..1` |
//! | Dynamics | single: `set_bow(v, v)` (speed & pressure) · section: — |
//! | Motion | `set_vibrato(5.5 Hz, v·0.05)` |
//! | Articulation | `set_bow_position` — sul tasto ↔ ponticello |
//! | Chairs | section: `set_desks` · single: — |
//! | Spread | section: `set_spread` (cents) · single: — |
//! | Width | section: `set_width` · single: — |
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use bowed_core::{Bowed, BowedEnsemble};

use crate::target::{chairs_count, MOTION_RATE_HZ, SPREAD_MAX_CENTS};
use crate::{clamp01, MacroId, MacroTarget};

/// Vibrato depth full scale (the core clamps bowed vibrato at 0.05).
const VIB_DEPTH_MAX: f32 = 0.05;

impl MacroTarget for Bowed {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Timbre => self.set_brightness(v),
            // Harder = faster & heavier bow (speed and pressure track together).
            MacroId::Dynamics => self.set_bow(v, v),
            MacroId::Motion => self.set_vibrato(MOTION_RATE_HZ, v * VIB_DEPTH_MAX),
            MacroId::Articulation => self.set_bow_position(v),
            MacroId::Chairs | MacroId::Spread | MacroId::Width => {}
        }
    }

    fn supports(&self, id: MacroId) -> bool {
        matches!(
            id,
            MacroId::Timbre | MacroId::Dynamics | MacroId::Motion | MacroId::Articulation
        )
    }
}

impl MacroTarget for BowedEnsemble {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Timbre => self.set_brightness(v),
            // The section's bow energy is a per-note property of the trigger
            // event, not a live setter — Dynamics is unsupported here.
            MacroId::Dynamics => {}
            MacroId::Motion => self.set_vibrato(MOTION_RATE_HZ, v * VIB_DEPTH_MAX),
            MacroId::Articulation => self.set_bow_position(v),
            MacroId::Chairs => self.set_desks(chairs_count(v)),
            MacroId::Spread => self.set_spread(v * SPREAD_MAX_CENTS),
            MacroId::Width => self.set_width(v),
        }
    }

    fn supports(&self, id: MacroId) -> bool {
        !matches!(id, MacroId::Dynamics)
    }
}
