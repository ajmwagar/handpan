//! The **expander bus** — an instrument-agnostic `{macro → value}` message set,
//! plus the per-module [`MacroEngine`] that produces one from conditioned CV.
//!
//! [`MacroBus`] is modeled exactly the way a VCV Rack expander passes a fixed
//! message struct across the base↔expander boundary: a small, `Copy`, POD-ish,
//! **alloc-free, fixed-size** value that one universal expander fills and every
//! family base consumes. The same struct also fits a poly-CV cable carrying the
//! macros as channels. Nothing here allocates or touches the heap, so it drops
//! straight onto firmware.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use crate::cv::{CvInput, Slew};
use crate::{clamp01, MacroId, MACRO_COUNT};

/// A single macro update — the atomic `{macro_id, value}` message. A stream of
/// these (a poly-CV cable, a control-change queue) can drive a [`MacroBus`] via
/// [`MacroBus::apply`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MacroMessage {
    /// Which macro this addresses.
    pub id: MacroId,
    /// Its new normalized value (`0..1`; clamped on apply).
    pub value: f32,
}

impl MacroMessage {
    /// A `{macro, value}` message.
    pub fn new(id: MacroId, value: f32) -> Self {
        MacroMessage { id, value }
    }
}

/// A fixed-size snapshot of all seven macros — the message an expander hands to
/// a base. Indexable by [`MacroId`]; every value is normalized `0..1`.
///
/// A per-macro **present** bit records whether the expander is actually driving
/// that macro (a patched channel) versus leaving it to the base's own panel.
/// [`crate::MacroTarget::apply_bus`] applies only the present macros, so an
/// expander that only touches, say, `Timbre` and `Width` leaves the rest alone.
#[derive(Clone, Copy, Debug)]
pub struct MacroBus {
    values: [f32; MACRO_COUNT],
    present: u8,
}

impl Default for MacroBus {
    fn default() -> Self {
        MacroBus::new()
    }
}

impl MacroBus {
    /// An empty bus: every macro at its neutral resting value and *not present*
    /// (so applying it is a no-op until something sets a macro).
    pub fn new() -> Self {
        MacroBus { values: [0.5; MACRO_COUNT], present: 0 }
    }

    /// Set a macro's value and mark it present.
    #[inline]
    pub fn set(&mut self, id: MacroId, value: f32) {
        self.values[id.index()] = clamp01(value);
        self.present |= 1 << id.index();
    }

    /// Apply a [`MacroMessage`] (same as [`set`](MacroBus::set)).
    #[inline]
    pub fn apply(&mut self, msg: MacroMessage) {
        self.set(msg.id, msg.value);
    }

    /// A macro's current value (its resting value if never set).
    #[inline]
    pub fn get(&self, id: MacroId) -> f32 {
        self.values[id.index()]
    }

    /// Whether a macro is being driven (was [`set`](MacroBus::set) since the last
    /// [`clear`](MacroBus::clear)).
    #[inline]
    pub fn is_present(&self, id: MacroId) -> bool {
        self.present & (1 << id.index()) != 0
    }

    /// Clear a single macro's present bit (stop driving it; leaves the value).
    #[inline]
    pub fn clear(&mut self, id: MacroId) {
        self.present &= !(1 << id.index());
    }

    /// Clear every present bit (nothing driven).
    #[inline]
    pub fn clear_all(&mut self) {
        self.present = 0;
    }

    /// Force every macro present at its current value (e.g. a base with no
    /// expander that still wants its own panel to drive all seven).
    #[inline]
    pub fn set_all_present(&mut self) {
        self.present = (1 << MACRO_COUNT) - 1;
    }
}

impl core::ops::Index<MacroId> for MacroBus {
    type Output = f32;
    #[inline]
    fn index(&self, id: MacroId) -> &f32 {
        &self.values[id.index()]
    }
}

/// A module's macro front panel: one [`CvInput`] (knob + attenuverter + normalled
/// CV) and one [`Slew`] limiter per macro. Feed it the conditioned CV for each
/// macro (unipolar `0..1`, or `None` when unpatched) and it returns a slewed
/// [`MacroBus`] ready to hand to a [`crate::MacroTarget`].
///
/// This is the "voice module" role: a base with its own knobs. The separate
/// "universal expander" role can build a [`MacroBus`] the same way and ship it
/// across the expander boundary — same struct, either source.
pub struct MacroEngine {
    inputs: [CvInput; MACRO_COUNT],
    slews: [Slew; MACRO_COUNT],
}

impl MacroEngine {
    /// Build with every knob at noon, attenuverters open, and a gentle default
    /// slew (`slew_ms`, e.g. ~10 ms) on every macro.
    pub fn new(fs: f32, slew_ms: f32) -> Self {
        let inputs = [CvInput::default(); MACRO_COUNT];
        let slews = [Slew::new(fs, slew_ms); MACRO_COUNT];
        let mut e = MacroEngine { inputs, slews };
        // Prime each slew at its knob so the first block starts settled, not at 0.
        for id in MacroId::ALL {
            let base = e.inputs[id.index()].eval(None);
            e.slews[id.index()].reset(base);
        }
        e
    }

    /// The [`CvInput`] for a macro (set its knob/attenuverter).
    #[inline]
    pub fn input_mut(&mut self, id: MacroId) -> &mut CvInput {
        &mut self.inputs[id.index()]
    }

    /// Set a macro's knob (`0..1`).
    #[inline]
    pub fn set_knob(&mut self, id: MacroId, knob: f32) {
        self.inputs[id.index()].set_knob(knob);
    }

    /// Set a macro's attenuverter (`-1..1`).
    #[inline]
    pub fn set_atten(&mut self, id: MacroId, atten: f32) {
        self.inputs[id.index()].set_atten(atten);
    }

    /// Retune a macro's slew glide time.
    #[inline]
    pub fn set_slew_ms(&mut self, id: MacroId, fs: f32, time_ms: f32) {
        self.slews[id.index()].set_time(fs, time_ms);
    }

    /// Advance one control step. `cvs[i]` is the conditioned CV for
    /// `MacroId::from_index(i)` — `Some(unipolar 0..1)` when patched, `None`
    /// when the jack is open (normalled to the knob). Returns a slewed
    /// [`MacroBus`] with every macro present.
    pub fn process(&mut self, cvs: &[Option<f32>; MACRO_COUNT]) -> MacroBus {
        let mut bus = MacroBus::new();
        for id in MacroId::ALL {
            let target = self.inputs[id.index()].eval(cvs[id.index()]);
            let slewed = self.slews[id.index()].process(target);
            bus.set(id, slewed);
        }
        bus
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn bus_routes_each_message_to_its_own_macro() {
        let mut bus = MacroBus::new();
        // Nothing is present on a fresh bus.
        for id in MacroId::ALL {
            assert!(!bus.is_present(id));
        }
        // A message reaches exactly its macro, and only it.
        bus.apply(MacroMessage::new(MacroId::Width, 0.75));
        assert!(bus.is_present(MacroId::Width));
        assert!((bus.get(MacroId::Width) - 0.75).abs() < 1e-6);
        assert!((bus[MacroId::Width] - 0.75).abs() < 1e-6);
        for id in MacroId::ALL {
            if id != MacroId::Width {
                assert!(!bus.is_present(id), "{:?} leaked present", id);
            }
        }
        // Values clamp on set.
        bus.set(MacroId::Timbre, 4.0);
        assert_eq!(bus.get(MacroId::Timbre), 1.0);
        // Clearing drops the present bit but keeps the value.
        bus.clear(MacroId::Width);
        assert!(!bus.is_present(MacroId::Width));
        assert!((bus.get(MacroId::Width) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn engine_conditions_knob_and_cv_into_a_slewed_bus() {
        let fs = 48_000.0;
        let mut eng = MacroEngine::new(fs, 5.0);
        eng.set_knob(MacroId::Timbre, 0.2);
        eng.set_atten(MacroId::Timbre, 1.0);

        // Unpatched everywhere: after settling, each macro rests at its knob.
        let open = [None; MACRO_COUNT];
        let mut bus = MacroBus::new();
        for _ in 0..48_000 {
            bus = eng.process(&open);
        }
        assert!((bus.get(MacroId::Timbre) - 0.2).abs() < 1e-3);
        assert!(bus.is_present(MacroId::Timbre));

        // Patch a full CV into Timbre: it climbs toward knob + atten·cv = 1.2 → 1.
        let mut cvs = [None; MACRO_COUNT];
        cvs[MacroId::Timbre.index()] = Some(1.0);
        let mut last = 0.0;
        for _ in 0..48_000 {
            last = eng.process(&cvs).get(MacroId::Timbre);
        }
        assert!(last > 0.99, "patched CV did not raise Timbre: {last}");
    }
}
