# handpan

Modal physical-modeling handpan — a VSTi and modular-firmware instrument.

The sound is generated, not sampled: each tone field is a bank of tuned modal
resonators (1:2:3 partials) with mode-split shimmer, an amplitude-dependent
pitch bloom, geometric-nonlinearity harmonic generation (hard strikes bloom
brighter — the "distortion process" measured in handpans/steelpans), a metallic
attack transient, harmonic-weighted sympathetic coupling, and a ~82 Hz center
cavity resonance. The voice is dependency-free and `no_std`-ready so the same
DSP compiles to both a plugin and embedded firmware.

Modal structure and the ~82 Hz cavity value follow Rossing, Morrison, Hansen,
Rohner & Schärer, *Acoustics of the Hang* / *Modes of Vibration and Sound
Radiation from the Hang* (Archives of Acoustics, 2007).

**Presets** are `build × size`: a dimpled/domed **Handpan** or a cut
**Tongue Drum**, each in Small / Standard / Large / Bass. See `VoiceProfile`.

**Scales**: D Kurd, D Celtic Minor, C# Kurd, D Hijaz, D Minor Pentatonic,
D Major (all 9-note), plus `Scale::Custom` for any MIDI tuning.

**Playability**: hand-mute damping (`damp`/`damp_all`, exposed as "Damp on
Release"), a built-in room "Air" ambience, and per-strike micro-variation so
repeats never sound mechanical.

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
