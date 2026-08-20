//! Plucked family adapter: maps the universal macros onto the three plucked
//! voices — [`Cifteli`], [`Gayageum`], and [`Basitar`]. These are distinct
//! single instruments (each with its own poly pool or string pair), so the
//! section macros Chairs/Spread mostly don't apply.
//!
//! Common ground: Timbre = string brightness, Dynamics = sustain (ring length).
//! From there each instrument's character comes through:
//!
//! | Macro | Cifteli | Gayageum | Basitar |
//! |---|---|---|---|
//! | Timbre | `set_brightness` | `set_brightness` | `set_brightness` |
//! | Dynamics | `set_sustain` | `set_sustain` | `set_sustain` |
//! | Motion | — | `set_vibrato(5.5, v·80c)` nonghyeon | — |
//! | Articulation | — | `set_bend(v·200c)` pitch push | `set_drive` amp grind |
//! | Chairs | — | — | — |
//! | Spread | — | — | — |
//! | Width | — | `set_width` | `set_width` |
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use plucked_core::{Basitar, Cifteli, Gayageum};

use crate::target::MOTION_RATE_HZ;
use crate::{clamp01, MacroId, MacroTarget};

/// Gayageum nonghyeon vibrato depth full scale, in cents (30–80c is idiomatic).
const GAYAGEUM_VIB_MAX_CENTS: f32 = 80.0;
/// Gayageum bend (left-hand pitch push) full scale, in cents.
const GAYAGEUM_BEND_MAX_CENTS: f32 = 200.0;

impl MacroTarget for Cifteli {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Timbre => self.set_brightness(v),
            MacroId::Dynamics => self.set_sustain(v),
            // A small bright lute: no vibrato, bend, section or stereo controls.
            MacroId::Motion
            | MacroId::Articulation
            | MacroId::Chairs
            | MacroId::Spread
            | MacroId::Width => {}
        }
    }

    fn supports(&self, id: MacroId) -> bool {
        matches!(id, MacroId::Timbre | MacroId::Dynamics)
    }
}

impl MacroTarget for Gayageum {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Timbre => self.set_brightness(v),
            MacroId::Dynamics => self.set_sustain(v),
            MacroId::Motion => self.set_vibrato(MOTION_RATE_HZ, v * GAYAGEUM_VIB_MAX_CENTS),
            MacroId::Articulation => self.set_bend(v * GAYAGEUM_BEND_MAX_CENTS),
            MacroId::Width => self.set_width(v),
            // One zither with its own overlapping string pool.
            MacroId::Chairs | MacroId::Spread => {}
        }
    }

    fn supports(&self, id: MacroId) -> bool {
        !matches!(id, MacroId::Chairs | MacroId::Spread)
    }
}

impl MacroTarget for Basitar {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Timbre => self.set_brightness(v),
            MacroId::Dynamics => self.set_sustain(v),
            // Amp grind is the basitar's attack character.
            MacroId::Articulation => self.set_drive(v),
            MacroId::Width => self.set_width(v),
            // No vibrato; the string interval is tuning, not a section spread.
            MacroId::Motion | MacroId::Chairs | MacroId::Spread => {}
        }
    }

    fn supports(&self, id: MacroId) -> bool {
        matches!(
            id,
            MacroId::Timbre | MacroId::Dynamics | MacroId::Articulation | MacroId::Width
        )
    }
}
