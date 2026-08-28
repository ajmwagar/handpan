# Puget Handpan — Daisy Patch.SM / Patch.Init() firmware

The real hardware firmware: `handpan-core` bound to the Daisy Patch.SM via the
`daisy` BSP. The audio DMA callback renders the stereo handpan; a control loop
reads the pots and gates. Builds to a flashable `thumbv7em-none-eabihf` binary
(text ~53 KB of 128 KB flash; 200 KB heap in AXI SRAM).

Unlike the `firmware/` skeleton (stubbed I/O), this drives the actual codec,
ADCs, and gate pins.

## Patch.Init() mapping

| Panel | Pin | Function |
|-------|-----|----------|
| Knob 1 | CV_1 (PIN_C5) | Scale (8 named) |
| Knob 2 | CV_2 (PIN_C4) | Size (S/Std/L/Bass) |
| Knob 3 | CV_3 (PIN_C3) | Air |
| Knob 4 | CV_4 (PIN_C2) | Damp (palm-mute) |
| Gate In 1 | PIN_B10 | Strike / clock the generative |
| Gate In 2 | PIN_B9 | Gu (bass hit) |
| Audio Out L/R | — | The instrument |
| User LED | PIN_C7 | Strike activity |

With no gate patched, GATE_IN_1 idle → the internal generative (Wander) plays
itself. V/Oct-in quantization and the remaining expression CVs (Velocity,
Position, Artic) are the next wiring pass.

## Build & flash

```sh
rustup target add thumbv7em-none-eabihf
cd firmware-daisy
cargo build --release
# ELF -> target/thumbv7em-none-eabihf/release/handpan-patch-init

# Flash over USB (Daisy bootloader / DFU):
cargo objcopy --release -- -O binary handpan.bin
dfu-util -a 0 -s 0x08000000:leave -D handpan.bin
# ...or `cargo run --release` with probe-rs + a debug probe.
```

## License

Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.
