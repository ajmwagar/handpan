/**
 * handpan_bridge.h — C ABI exported by the Rust `handpan-dsp` static library
 * (see dsp/src/lib.rs). The firmware links `libhandpan_dsp.a` and drives the
 * handpan-core voice through these calls.
 *
 * Unlike a pedal, the handpan is a *generator*: `pk_process_stereo` writes
 * stereo output (audio input is ignored) and note events arrive via
 * `pk_strike` / `pk_strike_gu` / `pk_clock` from the trigger jacks.
 *
 * Concurrency: `pk_process_stereo` runs in the audio interrupt; the control /
 * strike calls run in the main control loop — in-place field writes, so a
 * preempted update costs at worst one glitched sample.
 */
#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>

extern "C" {

/** Init allocator (heap owned by caller — put it in SDRAM) + build the voice
 *  for `sample_rate` Hz. Call once after hw.Init(). Returns 0 on success. */
int32_t pk_init(float sample_rate, uint8_t* heap, size_t heap_len);

/** Render one stereo block of `n` frames. Call from the audio callback. */
void pk_process_stereo(float* out_l, float* out_r, size_t n);

/** Number of pot-mapped controls, and their labels (dynamic pot binding). */
size_t pk_num_controls(void);
size_t pk_control_label(size_t idx, uint8_t* buf, size_t buf_len);
void   pk_set_control_by_index(size_t idx, float value);

/** Note events (from trigger jacks / CV). `artic`: 0 open, 1 mute, 2 slap;
 *  `position`: 0 center .. 1 edge; `volts`: 1V/oct, 0 = ding. */
void pk_strike(float volts, float velocity, uint32_t artic, float position);
void pk_strike_gu(float velocity);
void pk_clock(float velocity);
void pk_set_play_mode(uint32_t mode);

/** Field count of the current scale (for LED-ring note feedback). */
size_t pk_field_count(void);

} // extern "C"

namespace pk {
inline void ControlLabel(size_t idx, char* buf, size_t buf_len) {
    if (buf_len == 0) return;
    size_t n = pk_control_label(idx, reinterpret_cast<uint8_t*>(buf), buf_len - 1);
    if (n > buf_len - 1) n = buf_len - 1;
    buf[n] = '\0';
}
} // namespace pk
