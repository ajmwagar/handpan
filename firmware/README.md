# Handpan firmware (Daisy Patch.SM)

Two Eurorack modules off the shared `handpan-core` — the Unix-style split:

| Module | Role | In | Out |
|--------|------|----|-----|
| **handpan-voice** | Pure instrument voice | V/Oct (scale-quantized), Strike gate, knobs (Scale, Size/Build, Damp, Air) | Stereo L/R |
| **handpan-brain** | Generative sequencer, no audio | Clock, knobs (Scale, Mode, Density, Feel) | Gate + 1V/oct CV |

Patch the brain's Gate+CV into the voice's Strike+V/Oct and it plays itself;
or drive the voice from any sequencer/quantizer; or clock the voice's own
(optional) internal player. The voice is a good Eurorack citizen first.

## Status

- **DSP + musical logic**: complete and host-tested in `handpan-core` (voice,
  modal timbre, sequencer, quantization) and it **compiles and links for the
  MCU** — `cargo build --release` here produces real `thumbv7em-none-eabihf`
  ELF binaries for both modules.
- **Board I/O**: the `board` module in `src/lib.rs` is the single place the
  Daisy HAL binds to physical ADC/DAC/gate pins. It is stubbed so the modules
  link; wire it to the `daisy`/`libdaisy` HAL and the audio callback on
  hardware (the DSP it calls is already proven).

## Build

```sh
rustup target add thumbv7em-none-eabihf
cd firmware
cargo build --release
# -> target/thumbv7em-none-eabihf/release/handpan-voice
# -> target/thumbv7em-none-eabihf/release/handpan-brain
```

Flashing (once the HAL I/O is bound) uses the usual Daisy path — DFU over USB
or a probe:

```sh
cargo objcopy --release --bin handpan-voice -- -O binary handpan-voice.bin
dfu-util -a 0 -s 0x08000000:leave -D handpan-voice.bin   # or your bootloader
```

## Panel (8–10 HP, Patch.SM)

Voice: `CV_5`→V/Oct, `GATE_IN_1`→Strike, `CV_6`→Velocity, four knobs →
Scale / Size+Build / Damp / Air, `OUT_L`/`OUT_R`→ stereo.
Brain: `GATE_IN_2`→Clock, knobs → Scale / Mode / Density / Feel,
`CV_OUT_1`→Gate, `CV_OUT_2`→1V/oct.

The pin names above map to `board::*` in `src/lib.rs`.

## License

Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.
