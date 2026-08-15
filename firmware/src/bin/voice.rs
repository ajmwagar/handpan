//! Handpan **voice** module (Daisy Patch.SM) — a pure, composable instrument.
//!
//! Inputs: V/Oct (scale-quantized to tone fields), Strike gate, knobs
//! (Scale, Size/Build, Damp, Air). Output: stereo L/R.
//!
//! No sequencer here — this is the Unix-style voice you drive from any
//! sequencer/quantizer (or from the companion handpan-brain module).
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use panic_halt as _;

use handpan_core::{Build, HandpanInstrument, PlayMode, Scale, Size};
use handpan_firmware::{board, build_from_knob, init_heap, scale_from_knob, size_from_knob, Edge};

const FS: f32 = 48_000.0;

#[entry]
fn main() -> ! {
    init_heap();

    let mut scale = Scale::DKurd9;
    let mut build = Build::Handpan;
    let mut size = Size::Standard;
    let mut inst = HandpanInstrument::new(FS, &scale, build, size);
    inst.set_mode(PlayMode::Manual); // pure voice; the brain is a separate module

    let mut strike = Edge::new();

    loop {
        // Control rate: read the panel.
        let (scale_k, sizebuild_k, _damp_k, air_k) = board::read_knobs();
        inst.set_air(air_k);

        // Rebuild the voice only when scale/size/build actually change.
        let new_scale = scale_from_knob(scale_k);
        let new_size = size_from_knob(sizebuild_k);
        let new_build = build_from_knob(sizebuild_k);
        if new_scale.name() != scale.name() || new_size != size || new_build != build {
            scale = new_scale;
            size = new_size;
            build = new_build;
            inst.reconfigure(&scale, build, size);
        }

        // Strike the quantized V/oct field on a gate rising edge.
        if strike.rising(board::gate_high()) {
            inst.strike_voct(board::read_voct(), board::read_velocity());
        }

        // Audio block.
        for _ in 0..board::BLOCK {
            let (l, r) = inst.process();
            board::write_audio(l, r);
        }
    }
}
