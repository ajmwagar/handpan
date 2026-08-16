/**
 * pedal.cpp — Puget Handpan firmware for the Hermetic Modular Alchemy Lab.
 *
 * The whole Alchemy SDK surface is kept (a control page, pot-catch, param-lock
 * automation, per-knob CV, presets, settings, LED rings). The DSP is the Rust
 * `handpan-dsp` static library running the `handpan-core` modal voice.
 *
 * Adapted from the pedalkernel template. Two instrument-specific changes:
 *   - the audio callback is STEREO and a generator (pk_process_stereo — audio
 *     input ignored);
 *   - the trigger jacks J1/J2 play the instrument: J1 strikes the field at the
 *     V/Oct on CV_1 (with the internal generative self-clocking when J1 is
 *     idle), J2 hits the gu. The LED ring on pot 0 pulses on each strike.
 *
 * Controls (pk_num_controls, in order): Scale, Size, Air, Damp, Interact, Halo
 * — bound one-per-pot dynamically, so this file needs no edits to retune them.
 */

#include "daisy_seed.h"
#include "alchemy/hw/alchemy_lab.h"
#include "alchemy/surface/control_loop.h"
#include "alchemy/surface/cv_matrix.h"
#include "alchemy/surface/page.h"
#include "alchemy/surface/pager.h"
#include "alchemy/surface/param_lock.h"
#include "alchemy/surface/presets.h"
#include "alchemy/surface/settings.h"
#include "alchemy/surface/virtual_knob.h"

#include "handpan_bridge.h"
#include "pedal_palette.h"

using namespace alchemy;

/* Rust heap in SDRAM (64 MB). The modal voice + Air reverb allocate here. */
static constexpr size_t kPkHeapBytes = 4 * 1024 * 1024;
static uint8_t DSY_SDRAM_BSS pk_heap[kPkHeapBytes];

static VirtualKnob knobs[kNumPots];
static char        knob_names[kNumPots][24];
static uint8_t     g_num_controls = 0;

static Page            main_page(0);
static AlchemyLab      hw;
static ControlLoop     loop(hw);
static Pager           pager(hw.buttons[0], 1, kNumPots);
static ParamLock<kNumPots> locks(hw.buttons[0], pager);
static Presets         presets(hw.seed.qspi);
static Settings        settings(hw, &pager);
static CvMatrix        cv_matrix(kNumCvInputs);

/* Self-clock so the instrument plays itself when J1 is unpatched. ~500 ms at a
 * ~1 kHz control frame. Reset whenever an external trigger arrives. */
static constexpr uint32_t kSelfClockFrames = 500;
static uint32_t g_idle_frames = 0;
static uint8_t  g_strike_flash = 0;

static void UpdateControls()
{
    for (uint8_t i = 0; i < g_num_controls; ++i)
        pk_set_control_by_index(i, knobs[i].Value());

    /* Trigger jacks → note events. */
    bool struck = false;
    if (hw.j1.RisingEdge()) {
        pk_strike(hw.cv_jacks[0].Volts(), 0.85f, 0, 0.4f); // CV_1 = V/Oct
        struck = true;
        g_idle_frames = 0;
    }
    if (hw.j2.RisingEdge()) {
        pk_strike_gu(0.85f);
        struck = true;
    }
    /* Normalled generative: if J1 is idle, clock the internal player. */
    if (++g_idle_frames >= kSelfClockFrames) {
        pk_clock(0.7f);
        struck = true;
        g_idle_frames = 0;
    }
    if (struck) g_strike_flash = 6;

    /* Pot-0 ring pulses on a strike. */
    if (g_strike_flash > 0) g_strike_flash--;
}

/* Stereo generator: handpan-core writes both channels; input ignored. */
static void AudioCallback(daisy::AudioHandle::InputBuffer  in,
                          daisy::AudioHandle::OutputBuffer out,
                          size_t                           size)
{
    (void)in;
    pk_process_stereo(out[0], out[1], size);
}

int main()
{
    hw.Init();

    if (pk_init(hw.SampleRate(), pk_heap, sizeof(pk_heap)) != 0)
        for (;;) {}

    /* Discover the voice's controls and bind one pot per control, in order. */
    g_num_controls = static_cast<uint8_t>(pk_num_controls());
    if (g_num_controls > kNumPots)
        g_num_controls = kNumPots;

    for (uint8_t i = 0; i < g_num_controls; ++i)
    {
        pk::ControlLabel(i, knob_names[i], sizeof(knob_names[i]));
        knobs[i] = VirtualKnob(i, knob_names[i])
                       .Linear(0.f, 1.f)
                       .Ring(Level(kRingPalette[i % kRingPaletteLen], FillAnim::Pulse));
        main_page.Add(knobs[i]);
        cv_matrix.Jack(i).To(knobs[i]);
    }

    settings.UseBrightness();
    settings.UsePresets(presets);
    presets.Manage(pager);
    presets.Manage(locks);
    presets.Manage(settings);
    presets.Init();
    presets.BootLoad();

    /* Lower the trigger threshold a touch for weak strike sources. */
    hw.j1.SetTriggerThreshold(0.8f);
    hw.j2.SetTriggerThreshold(0.8f);

    UpdateControls();
    hw.StartAudio(AudioCallback);

    loop.Use(pager)
        .Use(locks)
        .Use(settings)
        .Use(cv_matrix)
        .Use(main_page)
        .OnFrame(UpdateControls);

    for (;;) loop.Tick();
}
