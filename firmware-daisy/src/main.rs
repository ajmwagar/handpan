//! Puget Handpan — Daisy Patch.SM / Patch.Init() firmware.
//!
//! The whole voice runs in `handpan-core`; this binds it to the real Daisy
//! hardware via the `daisy` BSP: the audio DMA callback renders the stereo
//! handpan, and a control loop reads the four pots and two gate inputs.
//!
//! Patch.Init() mapping: CV_1..CV_4 pots → Scale / Size / Air / Damp,
//! GATE_IN_1 → Strike (also clocks the internal generative), GATE_IN_2 → Gu,
//! Audio Out L/R → the instrument. With no gate patched it plays itself.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![no_main]
#![no_std]

use core::cell::RefCell;
use core::mem::MaybeUninit;

use cortex_m::interrupt::Mutex;
use cortex_m_rt::entry;
use panic_halt as _;

use embedded_alloc::LlffHeap as Heap;
use hal::adc;
use hal::delay::Delay;
use hal::pac::{self, interrupt};
use hal::prelude::*;
use stm32h7xx_hal as hal;

use daisy::audio;
use handpan_core::{scale::Scale, Build, HandpanInstrument, PlayMode, Size};

#[global_allocator]
static HEAP: Heap = Heap::empty();

// Heap lives in the 512 KB AXI SRAM (the modal/reverb buffers don't fit DTCM).
const HEAP_SIZE: usize = 200 * 1024;
#[link_section = ".sram"]
static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];

static AUDIO: Mutex<RefCell<Option<audio::Interface>>> = Mutex::new(RefCell::new(None));
static ENGINE: Mutex<RefCell<Option<HandpanInstrument>>> = Mutex::new(RefCell::new(None));

const FS: f32 = audio::FS.raw() as f32;

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

#[entry]
fn main() -> ! {
    unsafe { HEAP.init(core::ptr::addr_of_mut!(HEAP_MEM) as usize, HEAP_SIZE) }

    let mut cp = cortex_m::Peripherals::take().unwrap();
    let dp = pac::Peripherals::take().unwrap();
    cp.SCB.enable_icache();
    cp.SCB.enable_dcache(&mut cp.CPUID);

    let board = daisy::Board::take().unwrap();
    let ccdr = daisy::board_freeze_clocks!(board, dp);
    let pins = daisy::board_split_gpios!(board, ccdr, dp);
    let mut led = daisy::board_split_leds!(pins).USER;
    let audio_interface = daisy::board_split_audio!(ccdr, pins);

    // Build the instrument and hand it to the interrupt.
    let mut inst = HandpanInstrument::new(FS, &Scale::DHijaz9, Build::Handpan, Size::Large);
    inst.set_mode(PlayMode::Wander);
    inst.set_feel(0.22, 0.15);
    inst.set_air(0.18);

    let audio_interface = audio_interface.spawn().unwrap();
    cortex_m::interrupt::free(|cs| {
        AUDIO.borrow(cs).replace(Some(audio_interface));
        ENGINE.borrow(cs).replace(Some(inst));
    });

    // Controls.
    let mut delay = Delay::new(cp.SYST, ccdr.clocks);
    let mut adc1 = adc::Adc::adc1(
        dp.ADC1,
        4.MHz(),
        &mut delay,
        ccdr.peripheral.ADC12,
        &ccdr.clocks,
    )
    .enable();
    adc1.set_resolution(adc::Resolution::SixteenBit);

    let mut pot_scale = pins.GPIO.PIN_C5.into_analog(); // CV_1
    let mut pot_size = pins.GPIO.PIN_C4.into_analog(); // CV_2
    let mut pot_air = pins.GPIO.PIN_C3.into_analog(); // CV_3
    let mut pot_damp = pins.GPIO.PIN_C2.into_analog(); // CV_4
    let gate_strike = pins.GPIO.PIN_B10.into_pull_down_input(); // GATE_IN_1
    let gate_gu = pins.GPIO.PIN_B9.into_pull_down_input(); // GATE_IN_2

    let mut last_scale = 99u32;
    let mut last_size = 99u32;
    let mut last_strike = false;
    let mut last_gu = false;
    let mut clock_ticks = 0u32;

    loop {
        let rd = |v: u32| v as f32 / 65_535.0;
        let scale_i = (rd(adc1.read(&mut pot_scale).unwrap_or(0)) * 7.99) as u32;
        let size_i = (rd(adc1.read(&mut pot_size).unwrap_or(0)) * 3.99) as u32;
        let air = rd(adc1.read(&mut pot_air).unwrap_or(0)) * 0.6;
        let damp = rd(adc1.read(&mut pot_damp).unwrap_or(0));

        let strike = gate_strike.is_high();
        let gu = gate_gu.is_high();

        cortex_m::interrupt::free(|cs| {
            if let Some(e) = ENGINE.borrow(cs).borrow_mut().as_mut() {
                if scale_i != last_scale || size_i != last_size {
                    e.reconfigure(&scale_of(scale_i), Build::Handpan, size_of(size_i));
                    last_scale = scale_i;
                    last_size = size_i;
                }
                e.set_air(air);
                e.set_damp(damp);

                // GATE_IN_1 clocks the generative; if it's idle, self-clock so
                // it plays itself.
                clock_ticks += 1;
                if (strike && !last_strike) || clock_ticks >= 250 {
                    e.clock(0.7);
                    clock_ticks = 0;
                }
                if gu && !last_gu {
                    e.strike_gu(0.85);
                }
            }
        });
        last_strike = strike;
        last_gu = gu;

        if clock_ticks < 30 {
            led.set_high();
        } else {
            led.set_low();
        }
        delay.delay_ms(2u16);
    }
}

#[interrupt]
fn DMA1_STR1() {
    cortex_m::interrupt::free(|cs| {
        if let Some(interface) = AUDIO.borrow(cs).borrow_mut().as_mut() {
            let mut engine = ENGINE.borrow(cs).borrow_mut();
            interface
                .handle_interrupt_dma1_str1(|block| {
                    if let Some(e) = engine.as_mut() {
                        for frame in block.iter_mut() {
                            *frame = e.process();
                        }
                    }
                })
                .unwrap();
        }
    });
}
