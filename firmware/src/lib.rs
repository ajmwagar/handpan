//! Shared firmware support for the handpan Eurorack modules (Daisy Patch.SM).
//!
//! The DSP and musical logic live in `handpan-core` and are fully tested on the
//! host; this crate is the embedded shell: a global allocator, a gate
//! edge-detector, panel→parameter mapping, and a `board` I/O layer.
//!
//! The `board` functions are the single place the Daisy HAL binds to real
//! ADC/DAC/gate pins — they are stubbed here so the modules compile and link
//! for the MCU, and are wired to hardware on-device (see README).
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![no_std]

extern crate alloc;

use embedded_alloc::LlffHeap as Heap;
use handpan_core::{Build, Scale, Size};

#[global_allocator]
static HEAP: Heap = Heap::empty();

/// Initialize the heap the voice/brain use for their scale/mode vectors.
pub fn init_heap() {
    use core::mem::MaybeUninit;
    const HEAP_SIZE: usize = 64 * 1024;
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe {
        HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE);
    }
}

/// Rising-edge detector for gate/clock inputs.
pub struct Edge {
    last: bool,
}

impl Edge {
    pub const fn new() -> Self {
        Self { last: false }
    }
    /// Returns true on a low→high transition.
    pub fn rising(&mut self, high: bool) -> bool {
        let r = high && !self.last;
        self.last = high;
        r
    }
}

impl Default for Edge {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a knob in [0, 1] to one of the named scales.
pub fn scale_from_knob(v: f32) -> Scale {
    let all = Scale::all_named();
    let idx = ((v.clamp(0.0, 1.0) * all.len() as f32) as usize).min(all.len() - 1);
    all[idx].clone()
}

/// Map a knob in [0, 1] to an instrument size.
pub fn size_from_knob(v: f32) -> Size {
    match (v.clamp(0.0, 1.0) * 4.0) as usize {
        0 => Size::Small,
        1 => Size::Standard,
        2 => Size::Large,
        _ => Size::Bass,
    }
}

/// Map a knob in [0, 1] to a build (dimpled handpan vs cut tongue drum).
pub fn build_from_knob(v: f32) -> Build {
    if v < 0.5 {
        Build::Handpan
    } else {
        Build::TongueDrum
    }
}

/// Board I/O for the Daisy Patch.SM. These are stubs (wrapped in `black_box`
/// so the optimizer keeps the control flow); on hardware each binds to a
/// Patch.SM pin via the Daisy HAL. See README for the pin map.
pub mod board {
    use core::hint::black_box;

    /// Audio block size processed per control update.
    pub const BLOCK: usize = 48;

    // --- Inputs ---------------------------------------------------------
    /// 1V/oct pitch relative to the ding (CV_5 on the panel).
    pub fn read_voct() -> f32 {
        black_box(0.0)
    }
    /// Strike velocity 0..1 (CV_6, or a fixed level).
    pub fn read_velocity() -> f32 {
        black_box(0.8)
    }
    /// Strike gate high? (GATE_IN_1).
    pub fn gate_high() -> bool {
        black_box(false)
    }
    /// Clock gate high? (GATE_IN_2 on the brain).
    pub fn clock_high() -> bool {
        black_box(false)
    }
    /// The four control knobs (Scale, Size/Build, Damp, Air) in [0, 1].
    pub fn read_knobs() -> (f32, f32, f32, f32) {
        (black_box(0.0), black_box(0.25), black_box(0.0), black_box(0.16))
    }

    // --- Outputs --------------------------------------------------------
    /// Write one stereo audio sample (OUT_L / OUT_R).
    pub fn write_audio(l: f32, r: f32) {
        black_box((l, r));
    }
    /// Write the brain's 1V/oct CV output (CV_OUT_2).
    pub fn write_voct(v: f32) {
        black_box(v);
    }
    /// Write the brain's gate output (CV_OUT_1 / GATE_OUT), with velocity.
    pub fn write_gate(high: bool, velocity: f32) {
        black_box((high, velocity));
    }

    /// Idle between control ticks (a real build blocks on the audio callback).
    pub fn idle() {
        cortex_m::asm::nop();
    }
}
