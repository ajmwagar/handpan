//! handpan-dsp — the Rust ⇆ C++ bridge for the Alchemy Lab handpan firmware.
//!
//! Cross-compiles to `libhandpan_dsp.a` and exposes a small C ABI the C++
//! Alchemy SDK firmware links and drives. It's the instrument analog of the
//! `pk_*` pedal bridge: the C++ side keeps the whole SDK (pots, pages, CV,
//! presets, LED rings), and the audio callback calls `pk_process_stereo`.
//!
//! Unlike a pedal, the handpan is a *generator*: `pk_process_stereo` writes
//! stereo output and ignores audio input, and it takes note events
//! (`pk_strike` / `pk_strike_gu` / `pk_clock`) from gates/CV.
//!
//! Controls are exposed by index/label so the template's dynamic pot-binding
//! (and CV routing, presets, LED rings) works unchanged.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![no_std]

extern crate alloc;

use core::ptr::addr_of_mut;

use embedded_alloc::LlffHeap as Heap;
use handpan_core::{scale::Scale, Artic, Build, HandpanInstrument, PlayMode, Size};

use cortex_m as _;
use panic_halt as _;

#[global_allocator]
static HEAP: Heap = Heap::empty();

/// Live instrument plus the config we rebuild on scale/size change.
struct State {
    inst: HandpanInstrument,
    fs: f32,
    scale_idx: u32,
    size_idx: u32,
}

static mut STATE: Option<State> = None;

#[inline]
fn state_mut() -> Option<&'static mut State> {
    // SAFETY: bare-metal single-core; audio IRQ writes samples, control loop
    // writes params in place — the documented benign race from the template.
    unsafe { (*addr_of_mut!(STATE)).as_mut() }
}

/// Control layout (index → meaning), exposed to the C++ pot binder.
const LABELS: [&str; 6] = ["Scale", "Size", "Air", "Damp", "Interact", "Halo"];

fn scale_of(i: u32) -> Scale {
    Scale::all_named()[(i as usize).min(7)].clone()
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

/// Initialize the allocator (heap region owned by the C++ side — put it in
/// SDRAM) and build the instrument for `sample_rate` Hz. Returns 0 on success.
///
/// # Safety
/// `heap` must point to `heap_len` bytes of valid, exclusively-owned memory
/// that outlives all later calls.
#[no_mangle]
pub unsafe extern "C" fn pk_init(sample_rate: f32, heap: *mut u8, heap_len: usize) -> i32 {
    if heap.is_null() || heap_len == 0 {
        return -1;
    }
    unsafe { HEAP.init(heap as usize, heap_len) };
    let scale_idx = 3; // D Hijaz
    let size_idx = 2; // Large
    let mut inst =
        HandpanInstrument::new(sample_rate, &scale_of(scale_idx), Build::Handpan, size_of(size_idx));
    inst.set_mode(PlayMode::Wander);
    inst.set_feel(0.22, 0.15);
    inst.set_air(0.18);
    unsafe {
        STATE = Some(State { inst, fs: sample_rate, scale_idx, size_idx });
    }
    0
}

/// Render one stereo block of `n` frames (generator — audio input ignored).
/// Runs in the audio IRQ.
///
/// # Safety
/// `out_l`/`out_r` must each point to `n` valid, writable, non-overlapping f32s.
#[no_mangle]
pub unsafe extern "C" fn pk_process_stereo(out_l: *mut f32, out_r: *mut f32, n: usize) {
    match state_mut() {
        Some(s) => {
            for i in 0..n {
                let (l, r) = s.inst.process();
                unsafe {
                    *out_l.add(i) = l;
                    *out_r.add(i) = r;
                }
            }
        }
        None => {
            for i in 0..n {
                unsafe {
                    *out_l.add(i) = 0.0;
                    *out_r.add(i) = 0.0;
                }
            }
        }
    }
}

/// Number of controls (for dynamic pot binding).
#[no_mangle]
pub extern "C" fn pk_num_controls() -> usize {
    LABELS.len()
}

/// Copy control `idx`'s label into `buf` (not NUL-terminated); returns full len.
///
/// # Safety
/// `buf` must point to `buf_len` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn pk_control_label(idx: usize, buf: *mut u8, buf_len: usize) -> usize {
    let Some(label) = LABELS.get(idx) else { return 0 };
    let bytes = label.as_bytes();
    let n = bytes.len().min(buf_len);
    if !buf.is_null() && n > 0 {
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf, n) };
    }
    bytes.len()
}

/// Set control `idx` to a normalized 0..1 value (from a pot + CV).
#[no_mangle]
pub extern "C" fn pk_set_control_by_index(idx: usize, value: f32) {
    let Some(s) = state_mut() else { return };
    let v = value.clamp(0.0, 1.0);
    match idx {
        0 => {
            let si = (v * 7.99) as u32;
            if si != s.scale_idx {
                s.scale_idx = si;
                s.inst.reconfigure(&scale_of(si), Build::Handpan, size_of(s.size_idx));
            }
        }
        1 => {
            let zi = (v * 3.99) as u32;
            if zi != s.size_idx {
                s.size_idx = zi;
                s.inst.reconfigure(&scale_of(s.scale_idx), Build::Handpan, size_of(zi));
            }
        }
        2 => s.inst.set_air(v * 0.6),
        3 => s.inst.set_damp(v),
        4 => s.inst.set_shell(v * 0.3),
        5 => s.inst.set_coupling(v * 0.3),
        _ => {}
    }
}

/// Strike the quantized field for a 1V/oct pitch (0 V = ding), with a playing
/// articulation (0 open / 1 mute / 2 slap) and position (0 center..1 edge).
#[no_mangle]
pub extern "C" fn pk_strike(volts: f32, velocity: f32, artic: u32, position: f32) {
    if let Some(s) = state_mut() {
        s.inst.strike_voct_artic(volts, velocity, artic_of(artic), position);
    }
}

/// Strike the gu (bottom-port bass hit).
#[no_mangle]
pub extern "C" fn pk_strike_gu(velocity: f32) {
    if let Some(s) = state_mut() {
        s.inst.strike_gu(velocity);
    }
}

/// Advance the internal generative player one clock (normalled "plays itself").
#[no_mangle]
pub extern "C" fn pk_clock(velocity: f32) {
    if let Some(s) = state_mut() {
        s.inst.clock(velocity);
    }
}

/// Set the internal play mode (0 manual .. 6 euclid).
#[no_mangle]
pub extern "C" fn pk_set_play_mode(mode: u32) {
    if let Some(s) = state_mut() {
        let m = match mode {
            1 => PlayMode::Up,
            2 => PlayMode::Down,
            3 => PlayMode::UpDown,
            4 => PlayMode::Random,
            5 => PlayMode::Wander,
            6 => PlayMode::Euclid,
            _ => PlayMode::Manual,
        };
        s.inst.set_mode(m);
    }
}

/// The field count of the current scale (for LED-ring mapping on the C++ side).
#[no_mangle]
pub extern "C" fn pk_field_count() -> usize {
    state_mut().map_or(0, |s| s.inst.field_count())
}

// Keep fs referenced (config introspection / future re-init).
#[no_mangle]
pub extern "C" fn pk_sample_rate() -> f32 {
    state_mut().map_or(0.0, |s| s.fs)
}
