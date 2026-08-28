//! cxx bridge between the `handpan-core` voice and the Puget Handpan VCV Rack
//! C++ module. The whole instrument runs in Rust; this exposes a small opaque
//! engine the C++ `process()` drives.
//!
//! Voltage convention: VCV audio is ±5 V. The voice outputs ~±1; the C++ side
//! scales by 5 on output. V/Oct is 1 V per octave, 0 V = the ding.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use handpan_core::{scale::Scale, Artic, Build, HandpanInstrument, PlayMode, Size};

#[cxx::bridge(namespace = "handpan")]
mod ffi {
    /// One stereo output sample.
    struct StereoFrame {
        l: f32,
        r: f32,
    }

    extern "Rust" {
        type HandpanEngine;

        /// Build an engine. `scale`/`build`/`size` are enum indices.
        fn new_handpan_engine(
            sample_rate: f64,
            scale: u32,
            build: u32,
            size: u32,
        ) -> Box<HandpanEngine>;

        /// Rebuild the voice for new scale/size/build (or sample rate).
        fn reconfigure(engine: &mut HandpanEngine, sample_rate: f64, scale: u32, build: u32, size: u32);

        /// One stereo sample.
        fn process(engine: &mut HandpanEngine) -> StereoFrame;

        /// Quantized strike: `volts` V/Oct (0 = ding), `artic` 0=open/1=mute/
        /// 2=slap, `position` 0=center..1=edge.
        fn strike(engine: &mut HandpanEngine, volts: f32, velocity: f32, artic: u32, position: f32);

        /// Gu (bottom-port bass hit).
        fn strike_gu(engine: &mut HandpanEngine, velocity: f32);

        /// Advance the internal generative sequencer one clock (normalled play).
        fn clock(engine: &mut HandpanEngine, velocity: f32);
        fn set_play_mode(engine: &mut HandpanEngine, mode: u32);

        fn set_air(engine: &mut HandpanEngine, wet: f32);
        fn set_damp(engine: &mut HandpanEngine, amount: f32);
        fn set_shell(engine: &mut HandpanEngine, amount: f32);
        fn set_coupling(engine: &mut HandpanEngine, amount: f32);

        fn field_count(engine: &HandpanEngine) -> u32;
        fn scale_count() -> u32;
    }
}

pub struct HandpanEngine {
    inst: HandpanInstrument,
}

fn scale_of(i: u32) -> Scale {
    let all = Scale::all_named();
    all[(i as usize).min(all.len() - 1)].clone()
}

fn build_of(i: u32) -> Build {
    if i == 0 {
        Build::Handpan
    } else {
        Build::TongueDrum
    }
}

fn size_of(i: u32) -> Size {
    match i {
        0 => Size::Small,
        1 => Size::Standard,
        2 => Size::Large,
        _ => Size::Bass,
    }
}

fn artic_of(i: u32) -> Artic {
    match i {
        1 => Artic::Mute,
        2 => Artic::Slap,
        _ => Artic::Open,
    }
}

fn mode_of(i: u32) -> PlayMode {
    match i {
        1 => PlayMode::Up,
        2 => PlayMode::Down,
        3 => PlayMode::UpDown,
        4 => PlayMode::Random,
        5 => PlayMode::Wander,
        6 => PlayMode::Euclid,
        _ => PlayMode::Manual,
    }
}

fn new_handpan_engine(sample_rate: f64, scale: u32, build: u32, size: u32) -> Box<HandpanEngine> {
    let inst = HandpanInstrument::new(
        sample_rate as f32,
        &scale_of(scale),
        build_of(build),
        size_of(size),
    );
    Box::new(HandpanEngine { inst })
}

fn reconfigure(engine: &mut HandpanEngine, sample_rate: f64, scale: u32, build: u32, size: u32) {
    // HandpanInstrument keeps its own sample rate; rebuild fully to honor a
    // rate change too.
    engine.inst = HandpanInstrument::new(
        sample_rate as f32,
        &scale_of(scale),
        build_of(build),
        size_of(size),
    );
}

fn process(engine: &mut HandpanEngine) -> ffi::StereoFrame {
    let (l, r) = engine.inst.process();
    ffi::StereoFrame { l, r }
}

fn strike(engine: &mut HandpanEngine, volts: f32, velocity: f32, artic: u32, position: f32) {
    engine
        .inst
        .strike_voct_artic(volts, velocity, artic_of(artic), position);
}

fn strike_gu(engine: &mut HandpanEngine, velocity: f32) {
    engine.inst.strike_gu(velocity);
}

fn clock(engine: &mut HandpanEngine, velocity: f32) {
    engine.inst.clock(velocity);
}

fn set_play_mode(engine: &mut HandpanEngine, mode: u32) {
    engine.inst.set_mode(mode_of(mode));
}

fn set_air(engine: &mut HandpanEngine, wet: f32) {
    engine.inst.set_air(wet);
}
fn set_damp(engine: &mut HandpanEngine, amount: f32) {
    engine.inst.set_damp(amount);
}
fn set_shell(engine: &mut HandpanEngine, amount: f32) {
    engine.inst.set_shell(amount);
}
fn set_coupling(engine: &mut HandpanEngine, amount: f32) {
    engine.inst.set_coupling(amount);
}

fn field_count(engine: &HandpanEngine) -> u32 {
    engine.inst.field_count() as u32
}

fn scale_count() -> u32 {
    Scale::all_named().len() as u32
}
