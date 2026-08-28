//! Handpan **brain** module (Daisy Patch.SM) — a generative sequencer with no
//! audio. Clock in → Gate + 1V/oct CV out, walking a handpan scale. Patch it
//! into the handpan-voice module (or any voice) — the Unix-style other half.
//!
//! Inputs: Clock gate, knobs (Scale, Play mode, Density/Euclid, Feel).
//! Outputs: Gate + 1V/oct CV.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use panic_halt as _;

use handpan_core::{voct_of_field, PlayMode, Scale, Sequencer, Step};
use handpan_firmware::{board, init_heap, scale_from_knob, Edge};

#[entry]
fn main() -> ! {
    init_heap();

    let mut scale = Scale::DHijaz9;
    let mut freqs = scale.freqs();
    let mut seq = Sequencer::new(freqs.len(), 0xC0DE_CAFE);
    seq.set_mode(PlayMode::Wander);

    let mut clock = Edge::new();

    loop {
        // Control rate: scale select + Euclid density from knobs.
        let (scale_k, _mode_k, density_k, _feel_k) = board::read_knobs();
        let new_scale = scale_from_knob(scale_k);
        if new_scale.name() != scale.name() {
            scale = new_scale;
            freqs = scale.freqs();
            seq.set_fields(freqs.len());
        }
        let fill = 1 + (density_k.clamp(0.0, 1.0) * 7.0) as u8;
        seq.set_euclid(8, fill, 0);

        // On each clock, advance the sequencer and drive Gate + V/oct out.
        if clock.rising(board::clock_high()) {
            match seq.clock(board::read_velocity()) {
                Step::Strike { field, velocity } => {
                    board::write_voct(voct_of_field(&freqs, field));
                    board::write_gate(true, velocity);
                }
                Step::Rest => board::write_gate(false, 0.0),
            }
        }

        board::idle();
    }
}
