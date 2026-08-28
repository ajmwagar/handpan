# Puget Handpan — VCV Rack module

Software front-end for prototyping the handpan **in VCV Rack**, sharing the same
`handpan-core` voice as the plugin and the Daisy firmware. The panel mirrors the
Electrosmith **Patch.Init()** I/O so the control scheme carries straight to that
hardware.

## Architecture

```
handpan-core (Rust)  →  libvcv_handpan (staticlib + cxx bridge)  →  src/*.cpp (VCV module)  →  plugin.so
```

The whole instrument runs in Rust; the C++ is a thin wrapper mapping VCV I/O to
the engine over a `cxx` FFI bridge. Audio is scaled ±1 ↔ ±5 V; V/Oct is
1 V/oct with 0 V = the ding.

## Panel (12 HP) → Patch.Init() mapping

| Control | VCV | Patch.Init() |
|---|---|---|
| Scale (8 named) | knob | knob CV_1 |
| Size (S/Std/L/Bass) | knob | knob CV_2 |
| Air (room) | knob | knob CV_3 |
| Damp (palm-mute) | knob | knob CV_4 |
| V/Oct (quantized) | input | CV in (CV_5) |
| Velocity | input | CV in (CV_6) |
| Strike | gate in | Gate In 1 |
| Gu (bass) | gate in | Gate In 2 |
| Clock (internal player) | gate in | (shared / button) |
| L / R | outputs | Audio Out L/R |
| Build, Position, Artic, Shell, Mode | panel params | context menu / firmware config |

The Strike/Clock inputs drive the built-in generative player when nothing is
patched (the normalled "plays itself" behavior); patch a controller and it takes
over. V/Oct is scale-quantized — the built-in quantizer.

## Build

```sh
# One-time: fetch the Rack SDK (any 2.x)
curl -sSL -o rack-sdk.zip https://vcvrack.com/downloads/Rack-SDK-2.5.2-lin-x64.zip
mkdir -p .vcv-sdk && unzip -q rack-sdk.zip -d .vcv-sdk/

# Build the plugin
RACK_DIR=./.vcv-sdk/Rack-SDK make          # -> plugin.so
RACK_DIR=./.vcv-sdk/Rack-SDK make install   # -> ~/.Rack2/plugins/
```

Build artifacts (`.vcv-sdk/`, `build/`, `plugin.so`, the `.a`/`lib.rs.h`) are
git-ignored; only source is committed.

## License

Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.
