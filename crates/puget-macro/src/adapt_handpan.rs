//! Handpan family adapter: maps the universal macros onto [`HandpanInstrument`]
//! (the single modular voice) and [`HandpanEnsemble`] (the choir).
//!
//! The handpan is a *struck* instrument — no continuous exciter — so the macros
//! land on its resonant-body controls: shell nonlinearity (its metallic colour),
//! palm-mute damping (ring openness), room air (space/movement), and sympathetic
//! coupling (the struck halo). Section macros (Chairs/Spread/Width) apply to the
//! choir only; on the single voice they are unsupported no-ops.
//!
//! | Macro | Handpan parameter |
//! |---|---|
//! | Timbre | `set_shell` — shared-shell intermodulation (metallic combination tones), `0..0.4` |
//! | Dynamics | `set_damp(1 − v)` — more dynamics = more open ring |
//! | Motion | `set_air` — room ambience / spatial movement, `0..1` |
//! | Articulation | `set_coupling` — sympathetic halo between fields, `0..0.5` |
//! | Chairs | choir: `set_chairs` · single: — |
//! | Spread | choir: `set_spread` (cents) · single: — |
//! | Width | choir: `set_width` · single: — |
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use handpan_core::{HandpanEnsemble, HandpanInstrument};

use crate::target::{chairs_count, SPREAD_MAX_CENTS};
use crate::{clamp01, MacroId, MacroTarget};

/// Shell nonlinearity clamps at 0.4 in the core; keep Timbre inside that.
const SHELL_MAX: f32 = 0.4;
/// Coupling clamps at 0.5 in the core.
const COUPLING_MAX: f32 = 0.5;

impl MacroTarget for HandpanInstrument {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Timbre => self.set_shell(v * SHELL_MAX),
            MacroId::Dynamics => self.set_damp(1.0 - v),
            MacroId::Motion => self.set_air(v),
            MacroId::Articulation => self.set_coupling(v * COUPLING_MAX),
            // A single voice has no section.
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

impl MacroTarget for HandpanEnsemble {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        let v = clamp01(value);
        match id {
            MacroId::Timbre => self.set_shell(v * SHELL_MAX),
            MacroId::Dynamics => self.set_damp(1.0 - v),
            MacroId::Motion => self.set_air(v),
            MacroId::Articulation => self.set_coupling(v * COUPLING_MAX),
            MacroId::Chairs => self.set_chairs(chairs_count(v)),
            MacroId::Spread => self.set_spread(v * SPREAD_MAX_CENTS),
            MacroId::Width => self.set_width(v),
        }
    }

    fn supports(&self, _id: MacroId) -> bool {
        // The choir maps all seven.
        true
    }
}
