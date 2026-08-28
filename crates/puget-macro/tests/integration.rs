//! End-to-end macro-engine tests: a [`MacroBus`] routed through a
//! [`MacroTarget`] must actually move the intended parameter on real voices —
//! the audible proxy for each macro changes in the expected direction.
//!
//! These run on the host (`std`) with every family feature on (the default).
//! A `no_std` / firmware build is verified separately from the command line:
//!
//! ```text
//! cargo build -p puget-macro --no-default-features --features libm,handpan
//! cargo build -p puget-macro --no-default-features --features libm,wind
//! ```
//!
//! (one family at a time — proving each adapter compiles standalone for firmware).
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use puget_macro::{MacroBus, MacroId, MacroMessage, MacroTarget};

/// A mock target that just records which macro was applied with which value —
/// proves the bus routes `{macro_id, value}` to exactly the right macro.
#[derive(Default)]
struct Recorder {
    last: [Option<f32>; 7],
}
impl MacroTarget for Recorder {
    fn apply_macro(&mut self, id: MacroId, value: f32) {
        self.last[id.index()] = Some(value);
    }
    fn supports(&self, _id: MacroId) -> bool {
        true
    }
}

#[test]
fn bus_apply_routes_only_present_macros_to_their_slots() {
    let mut rec = Recorder::default();
    let mut bus = MacroBus::new();
    bus.apply(MacroMessage::new(MacroId::Motion, 0.42));
    bus.set(MacroId::Chairs, 0.9);
    rec.apply_bus(&bus);

    // Exactly the two set macros were applied, to their own slots.
    assert_eq!(rec.last[MacroId::Motion.index()], Some(0.42));
    assert_eq!(rec.last[MacroId::Chairs.index()], Some(0.9));
    for id in MacroId::ALL {
        if id != MacroId::Motion && id != MacroId::Chairs {
            assert_eq!(rec.last[id.index()], None, "{:?} should not have been applied", id);
        }
    }
}

#[test]
fn unsupported_macros_are_skipped_by_apply_bus() {
    // A target that supports nothing must receive nothing.
    struct Nothing(bool);
    impl MacroTarget for Nothing {
        fn apply_macro(&mut self, _id: MacroId, _v: f32) {
            self.0 = true;
        }
        fn supports(&self, _id: MacroId) -> bool {
            false
        }
    }
    let mut n = Nothing(false);
    let mut bus = MacroBus::new();
    for id in MacroId::ALL {
        bus.set(id, 0.5);
    }
    n.apply_bus(&bus);
    assert!(!n.0, "apply_bus called an unsupported macro");
}

// ── Width: stereo image widens (L/R difference grows) ────────────────────────

#[cfg(feature = "wind")]
#[test]
fn width_macro_widens_the_stereo_image() {
    use wind_core::{WindEnsemble, WindKind};
    let fs = 48_000.0;

    fn max_lr_diff(width01: f32) -> f32 {
        let fs = 48_000.0;
        let mut sec = WindEnsemble::new(fs, WindKind::Clarinet, 8, 7.0);
        let mut bus = MacroBus::new();
        bus.set(MacroId::Chairs, 0.7);
        bus.set(MacroId::Width, width01);
        sec.apply_bus(&bus);
        sec.note_on(261.63, 0.9);
        let mut diff = 0.0f32;
        for i in 0..fs as usize {
            let (l, r) = sec.process();
            if i > fs as usize / 2 {
                diff = diff.max((l - r).abs());
            }
        }
        diff
    }

    let narrow = max_lr_diff(0.0);
    let wide = max_lr_diff(1.0);
    assert!(wide > narrow, "Width macro did not widen the image: {narrow} -> {wide}");
    let _ = fs;
}

// ── Chairs: the active voice count changes ───────────────────────────────────

#[cfg(feature = "mallet")]
#[test]
fn chairs_macro_changes_the_active_voice_count() {
    use mallet_core::{Instrument, MalletEnsemble};
    let fs = 48_000.0;
    let mut sec = MalletEnsemble::new(fs, Instrument::Marimba, 12, 5.0, 4);

    let mut solo = MacroBus::new();
    solo.set(MacroId::Chairs, 0.0); // → 1 chair
    sec.apply_bus(&solo);
    let one = sec.chairs();

    let mut full = MacroBus::new();
    full.set(MacroId::Chairs, 1.0); // → the section's maximum
    sec.apply_bus(&full);
    let many = sec.chairs();

    assert_eq!(one, 1, "Chairs=0 should be a soloist, got {one}");
    assert!(many > one, "Chairs=1 should be a bigger section: {one} -> {many}");
}

// ── Timbre: spectral brightness increases ────────────────────────────────────

#[cfg(feature = "wind")]
#[test]
fn timbre_macro_brightens_the_spectrum() {
    use wind_core::{Wind, WindKind};

    // A spectral-tilt proxy: the energy in the first difference (a crude
    // high-pass) relative to the total. Brighter tone → more HF → higher ratio.
    fn brightness_proxy(timbre01: f32) -> f32 {
        let fs = 48_000.0;
        let mut v = Wind::new(fs, WindKind::Clarinet);
        let mut bus = MacroBus::new();
        bus.set(MacroId::Timbre, timbre01);
        bus.set(MacroId::Dynamics, 0.9); // steady breath so it sings
        v.apply_bus(&bus);
        v.note_on(261.63, 0.9);
        for _ in 0..24_000 {
            v.process();
        }
        let mut prev = 0.0f32;
        let mut hf = 0.0f64;
        let mut tot = 0.0f64;
        for _ in 0..16_384 {
            let x = v.process();
            let d = x - prev;
            prev = x;
            hf += (d * d) as f64;
            tot += (x * x) as f64;
        }
        (hf / tot.max(1e-12)) as f32
    }

    let dark = brightness_proxy(0.1);
    let bright = brightness_proxy(0.95);
    assert!(bright > dark, "Timbre macro did not brighten: dark={dark:.4} bright={bright:.4}");
}

// ── Dynamics: opening the ring on a struck voice ─────────────────────────────

#[cfg(feature = "handpan")]
#[test]
fn dynamics_macro_opens_the_handpan_ring() {
    use handpan_core::{scale::Scale, Build, HandpanInstrument, PlayMode, Size};

    // Dynamics → set_damp(1 − v): high Dynamics = open ring (long tail); low
    // Dynamics = heavily palm-muted (short tail).
    fn tail_energy(dynamics01: f32) -> f32 {
        let fs = 48_000.0;
        let mut hp = HandpanInstrument::new(fs, &Scale::DKurd9, Build::Handpan, Size::Standard);
        hp.set_mode(PlayMode::Manual);
        let mut bus = MacroBus::new();
        bus.set(MacroId::Dynamics, dynamics01);
        hp.apply_bus(&bus);
        hp.strike_field(0, 1.0);
        // Let it ring 1.5 s, then measure the remaining energy.
        for _ in 0..(fs as usize * 3 / 2) {
            hp.process();
        }
        let mut tail = 0.0f32;
        for _ in 0..4_800 {
            let (l, r) = hp.process();
            tail = tail.max(l.abs()).max(r.abs());
        }
        tail
    }

    let muted = tail_energy(0.0); // fully damped
    let open = tail_energy(1.0); // fully open
    assert!(open > muted, "Dynamics did not open the ring: muted={muted:.5} open={open:.5}");
}
