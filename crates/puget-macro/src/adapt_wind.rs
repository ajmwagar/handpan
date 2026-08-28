//! Wind family adapter: maps the universal macros onto [`Wind`] (a single
//! aerophone) and [`WindEnsemble`] (a wind section).
//!
//! Winds are self-sustained blown oscillators, so the macros are the breath /
//! embouchure expression surface: brightness, breath pressure, vibrato, growl.
//!
//! | Macro | Wind parameter |
//! |---|---|
//! | Timbre | `set_brightness` — loop cutoff / embouchure, `0..1` |
//! | Dynamics | `set_breath` — breath pressure, `0..1` |
//! | Motion | `set_vibrato(5.5 Hz, v·0.4)` — vibrato depth (didge: wobble) |
//! | Articulation | `set_growl(v·0.3, 5.5 Hz)` — throaty flutter |
//! | Chairs | section: `set_chairs` · single: — |
//! | Spread | section: `set_spread` (cents) · single: — |
//! | Width | section: `set_width` · single: — |
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use wind_core::{Wind, WindEnsemble};

use crate::target::{chairs_count, MOTION_RATE_HZ, SPREAD_MAX_CENTS};
use crate::{clamp01, MacroId, MacroTarget};

/// Vibrato depth full scale (the core clamps at 0.5; a musical max is ~0.4).
const VIB_DEPTH_MAX: f32 = 0.4;
/// Growl depth full scale (the core clamps at 0.5; keep it a subtle-to-throaty range).
const GROWL_DEPTH_MAX: f32 = 0.3;

impl MacroTarget for Wind {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Timbre => self.set_brightness(v),
            MacroId::Dynamics => self.set_breath(v),
            MacroId::Motion => self.set_vibrato(MOTION_RATE_HZ, v * VIB_DEPTH_MAX),
            MacroId::Articulation => self.set_growl(v * GROWL_DEPTH_MAX, MOTION_RATE_HZ),
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

impl MacroTarget for WindEnsemble {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Timbre => self.set_brightness(v),
            MacroId::Dynamics => self.set_breath(v),
            MacroId::Motion => self.set_vibrato(MOTION_RATE_HZ, v * VIB_DEPTH_MAX),
            MacroId::Articulation => self.set_growl(v * GROWL_DEPTH_MAX, MOTION_RATE_HZ),
            MacroId::Chairs => self.set_chairs(chairs_count(v)),
            MacroId::Spread => self.set_spread(v * SPREAD_MAX_CENTS),
            MacroId::Width => self.set_width(v),
        }
    }

    fn supports(&self, _id: MacroId) -> bool {
        true
    }
}
