# handpan

Modal physical-modeling handpan — a VSTi and modular-firmware instrument.

The sound is generated, not sampled: each tone field is a bank of tuned modal
resonators with harmonic octave/fifth tuning, mode-split shimmer, sympathetic
coupling, and a shell resonance. The voice is dependency-free and `no_std`-ready
so the same DSP compiles to both a plugin and embedded firmware.

## Layout

| Crate | What it is |
|-------|------------|
| `crates/handpan-core` | The voice. Pure modal DSP, **zero dependencies**, `no_std + alloc` capable. This is the IP. |
| `crates/handpan-plugin` | `Puget Handpan` — VST3/CLAP instrument (nih-plug) wrapping the core. |
| `xtask` | nih-plug bundler. |

## Build

```sh
# Run the tests and hear the voice (writes handpan_demo.wav)
cargo test -p handpan-core
cargo run -p handpan-core --example render_wav

# Build the plugin bundles -> target/bundled/Puget Handpan.{vst3,clap}
cargo xtask bundle handpan-plugin --release
```

## License

Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved. See `LICENSE`.
