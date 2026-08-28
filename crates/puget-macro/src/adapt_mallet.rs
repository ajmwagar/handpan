//! Mallet family adapter: maps the universal macros onto [`Mallet`] (a
//! polyphonic mallet/bell voice) and [`MalletEnsemble`] (a mallet section).
//!
//! Mallet percussion is struck and its spectral colour is baked into the chosen
//! instrument's mode data (marimba vs. glockenspiel vs. church bell), so there
//! is no continuous Timbre or Articulation control — those macros are
//! unsupported no-ops. Dynamics opens the ring (un-damps), and Motion is the
//! tremolo motor (the vibraphone's fan).
//!
//! | Macro | Mallet parameter |
//! |---|---|
//! | Timbre | — (fixed by the instrument's modes) |
//! | Dynamics | `set_damp(1 − v)` — ring openness |
//! | Motion | `set_tremolo(5.5 Hz, v)` — motor tremolo |
//! | Articulation | — |
//! | Chairs | section: `set_chairs` · single: — |
//! | Spread | section: `set_spread` (cents) · single: — |
//! | Width | section: `set_width` · single: — |
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use mallet_core::{Mallet, MalletEnsemble};

use crate::target::{chairs_count, MOTION_RATE_HZ, SPREAD_MAX_CENTS};
use crate::{clamp01, MacroId, MacroTarget};

impl MacroTarget for Mallet {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Dynamics => self.set_damp(1.0 - v),
            MacroId::Motion => self.set_tremolo(MOTION_RATE_HZ, v),
            // Struck, fixed-timbre, single voice: nothing else applies.
            MacroId::Timbre
            | MacroId::Articulation
            | MacroId::Chairs
            | MacroId::Spread
            | MacroId::Width => {}
        }
    }

    fn supports(&self, id: MacroId) -> bool {
        matches!(id, MacroId::Dynamics | MacroId::Motion)
    }
}

impl MacroTarget for MalletEnsemble {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Dynamics => self.set_damp(1.0 - v),
            MacroId::Motion => self.set_tremolo(MOTION_RATE_HZ, v),
            MacroId::Chairs => self.set_chairs(chairs_count(v)),
            MacroId::Spread => self.set_spread(v * SPREAD_MAX_CENTS),
            MacroId::Width => self.set_width(v),
            MacroId::Timbre | MacroId::Articulation => {}
        }
    }

    fn supports(&self, id: MacroId) -> bool {
        !matches!(id, MacroId::Timbre | MacroId::Articulation)
    }
}
