# Puget Handpan — Alchemy Lab firmware

Firmware for the [Hermetic Modular Alchemy Lab](https://hermeticmodular.com/modules/alchemy-lab)
whose DSP is the `handpan-core` modal voice, running on the C++ Alchemy SDK.
Modeled on [`alchemy-template-rust`](https://github.com/FuturePresentLabs/alchemy-template-rust),
but the voice is a polyphonic **instrument**, not a pedal.

```
┌──────────────────────── firmware (Cortex-M7) ────────────────────────┐
│  C++ (Alchemy SDK)                    Rust (handpan-core, no_std)     │
│  pots · pages · CV · presets          ┌───────────────────────────┐  │
│  LED rings · trigger jacks   ── pk_* ─►│ modal handpan voice       │  │
│  stereo audio callback  ◄─ pk_process_stereo │ (Air heap in SDRAM)  │  │
│  main()                               └───────────────────────────┘  │
│                                          libhandpan_dsp.a (linked)    │
└───────────────────────────────────────────────────────────────────────┘
```

## Layout

```
alchemy/
├── Makefile                 builds the Rust lib + firmware, links them
├── src/
│   ├── pedal.cpp            C++ firmware — pots, CV, presets, LED rings, audio cb
│   ├── handpan_bridge.h     the C ABI exported by the Rust lib
│   └── pedal_palette.h      LED ring colors
└── dsp/                     the Rust half → libhandpan_dsp.a
    ├── Cargo.toml           staticlib; depends on ../../crates/handpan-core
    ├── .cargo/config.toml   thumbv7em-none-eabihf, cortex-m7
    └── src/lib.rs           the bridge: pk_init / pk_process_stereo / pk_strike / …
```

`handpan-core` is a path dependency inside this repo, so — unlike the pedal
template — there is **no pedalkernel submodule**. You add only the two the C++
side needs:

```sh
git submodule add https://github.com/hermetic-modular/alchemy-sdk lib/alchemy-sdk
git submodule add https://github.com/electro-smith/libDaisy       lib/libDaisy
```

## Panel mapping (Alchemy Lab, 6 pots / 2 trigger jacks / stereo)

| Control | Source | Function |
|---|---|---|
| Pot 1 | control | Scale (8 named) |
| Pot 2 | control | Size |
| Pot 3 | control | Air |
| Pot 4 | control | Damp (palm-mute) |
| Pot 5 | control | Interact (shell intermodulation) |
| Pot 6 | control | Halo (sympathetic coupling) |
| J1 | trigger | Strike at CV_1 V/Oct (self-clocks the generative when idle) |
| J2 | trigger | Gu (bass hit) |
| CV_1 | CV | V/Oct for J1 strikes |
| Out L/R | audio | The instrument |
| LED rings | — | per-pot color; pot-1 ring pulses on strike |

Pots bind dynamically from the Rust side's `pk_num_controls`/`pk_control_label`,
so retuning the control set needs no C++ edits. The 102-LED panel can later show
the tone-field layout and which fields are ringing (`pk_field_count`).

## Build

```sh
rustup target add thumbv7em-none-eabihf
git submodule update --init          # alchemy-sdk + libDaisy
make libdaisy                        # once
make                                 # -> build/pedal.bin
make program-dfu                     # flash (module in DFU mode)
```

The Rust DSP library builds and is verified on its own with:
`cd dsp && cargo build --release` → `libhandpan_dsp.a`.

## License

Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.
