//! Puget Studio — a universal VST3/CLAP instrument that exposes every Puget
//! physical-modeling voice (winds, bowed strings, mallets, plucked strings,
//! frame/goblet drums, and the handpan) behind a single instrument selector,
//! playable from any DAW piano roll over MIDI. There is no sampled content and
//! no GUI yet (parameters only).
//!
//! MIDI note numbers are mapped to Hz with standard 12-TET
//! (`440 * 2^((n-69)/12)`); note velocity is the nih-plug 0..1 float. Each
//! voice family handles note-on / note-off according to its physics — see the
//! per-family notes on [`AnyInstrument`].
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::sync::Arc;

use nih_plug::prelude::*;

use bowed_core::{Bowed, StringKind};
use drum_core::{Bodhran, Daf, Dohol, Tombak};
use handpan_core::{scale::Scale, Build, Handpan, VoiceProfile};
use mallet_core::{Instrument as MalletInstrument, Mallet};
use plucked_core::{Barbat, Basitar, Cifteli, Gayageum, Santur, Setar, Tar};
use puget_dsp::Reverb;
use wind_core::{Wind, WindKind};

/// Convert a MIDI note number to frequency in Hz (12-TET, A4 = 440 Hz).
#[inline]
fn midi_to_hz(note: u8) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}

/// The single instrument selector, covering every voice the plugin exposes.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
enum InstrumentChoice {
    // --- Winds ---
    #[id = "clarinet"]
    #[name = "Clarinet"]
    Clarinet,
    #[id = "flute"]
    #[name = "Flute"]
    Flute,
    #[id = "saxophone"]
    #[name = "Saxophone"]
    Saxophone,
    #[id = "trumpet"]
    #[name = "Trumpet"]
    Trumpet,
    #[id = "didgeridoo"]
    #[name = "Didgeridoo"]
    Didgeridoo,
    #[id = "ney"]
    #[name = "Ney"]
    Ney,
    #[id = "sorna"]
    #[name = "Sorna"]
    Sorna,
    #[id = "tin_whistle"]
    #[name = "Tin Whistle"]
    TinWhistle,
    #[id = "irish_flute"]
    #[name = "Irish Flute"]
    IrishFlute,
    #[id = "uilleann_pipes"]
    #[name = "Uilleann Pipes"]
    UilleannPipes,
    // --- Bowed ---
    #[id = "violin"]
    #[name = "Violin"]
    Violin,
    #[id = "viola"]
    #[name = "Viola"]
    Viola,
    #[id = "cello"]
    #[name = "Cello"]
    Cello,
    #[id = "bass"]
    #[name = "Double Bass"]
    Bass,
    #[id = "kamancheh"]
    #[name = "Kamancheh"]
    Kamancheh,
    #[id = "fiddle"]
    #[name = "Fiddle"]
    Fiddle,
    // --- Mallets ---
    #[id = "marimba"]
    #[name = "Marimba"]
    Marimba,
    #[id = "xylophone"]
    #[name = "Xylophone"]
    Xylophone,
    #[id = "vibraphone"]
    #[name = "Vibraphone"]
    Vibraphone,
    #[id = "glockenspiel"]
    #[name = "Glockenspiel"]
    Glockenspiel,
    #[id = "tubular_bell"]
    #[name = "Tubular Bell"]
    TubularBell,
    #[id = "church_bell"]
    #[name = "Church Bell"]
    ChurchBell,
    #[id = "music_box"]
    #[name = "Music Box"]
    MusicBox,
    #[id = "singing_bowl"]
    #[name = "Singing Bowl"]
    SingingBowl,
    // --- Plucked ---
    #[id = "cifteli"]
    #[name = "Cifteli"]
    Cifteli,
    #[id = "gayageum"]
    #[name = "Gayageum"]
    Gayageum,
    #[id = "basitar"]
    #[name = "Basitar"]
    Basitar,
    #[id = "santur"]
    #[name = "Santur"]
    Santur,
    #[id = "tar"]
    #[name = "Tar"]
    Tar,
    #[id = "setar"]
    #[name = "Setar"]
    Setar,
    #[id = "barbat"]
    #[name = "Barbat"]
    Barbat,
    // --- Drums ---
    #[id = "dohol"]
    #[name = "Dohol"]
    Dohol,
    #[id = "tombak"]
    #[name = "Tombak"]
    Tombak,
    #[id = "daf"]
    #[name = "Daf"]
    Daf,
    #[id = "bodhran"]
    #[name = "Bodhran"]
    Bodhran,
    // --- Handpan family ---
    #[id = "handpan"]
    #[name = "Handpan"]
    Handpan,
    #[id = "tongue_drum"]
    #[name = "Tongue Drum"]
    TongueDrum,
}

/// A monophonic single-voice engine that the generic [`PolyPool`] can drive.
/// Implemented for the wind and bowed cores, which each sound one note at a
/// time; the pool stacks N of them for polyphony.
trait MonoVoice {
    /// Start a note. `expression` carries the family-specific articulation
    /// control (wind breath scaler / bow pressure).
    fn v_note_on(&mut self, freq: f32, velocity: f32, expression: f32);
    fn v_note_off(&mut self);
    fn v_process(&mut self) -> f32;
    fn v_set_brightness(&mut self, amount: f32);
    fn v_set_vibrato(&mut self, rate_hz: f32, depth: f32);
}

impl MonoVoice for Wind {
    fn v_note_on(&mut self, freq: f32, velocity: f32, expression: f32) {
        // Velocity drives breath; the Expression knob scales it around unity
        // (0.5 -> ~1.0x, the neutral default).
        let breath = (velocity * (0.5 + expression)).clamp(0.0, 1.5);
        self.note_on(freq, breath);
    }
    fn v_note_off(&mut self) {
        self.note_off();
    }
    fn v_process(&mut self) -> f32 {
        self.process()
    }
    fn v_set_brightness(&mut self, amount: f32) {
        self.set_brightness(amount);
    }
    fn v_set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        self.set_vibrato(rate_hz, depth);
    }
}

impl MonoVoice for Bowed {
    fn v_note_on(&mut self, freq: f32, velocity: f32, expression: f32) {
        // Velocity -> bow velocity; Expression -> bow pressure.
        self.note_on(freq, velocity.clamp(0.0, 1.0), expression.clamp(0.05, 1.0));
    }
    fn v_note_off(&mut self) {
        self.note_off();
    }
    fn v_process(&mut self) -> f32 {
        self.process()
    }
    fn v_set_brightness(&mut self, amount: f32) {
        self.set_brightness(amount);
    }
    fn v_set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        self.set_vibrato(rate_hz, depth);
    }
}

/// One allocation slot in a [`PolyPool`].
struct Slot<E> {
    engine: E,
    /// The MIDI note currently owning this slot, or `None` if free.
    note: Option<u8>,
    /// Allocation clock stamp, for oldest-first voice stealing.
    age: u64,
}

/// A generic voice allocator that stacks N monophonic [`MonoVoice`] engines
/// into a polyphonic instrument. Note-off lets the engine's release tail ring
/// out while its slot is freed for re-use.
struct PolyPool<E: MonoVoice> {
    slots: Vec<Slot<E>>,
    clock: u64,
    /// Family articulation control forwarded to every note-on (breath / bow
    /// pressure), refreshed once per block from the Expression param.
    expression: f32,
}

impl<E: MonoVoice> PolyPool<E> {
    /// Build a pool of `n` voices (minimum 1) from a factory closure.
    fn new(n: usize, mut factory: impl FnMut() -> E) -> Self {
        let slots = (0..n.max(1))
            .map(|_| Slot {
                engine: factory(),
                note: None,
                age: 0,
            })
            .collect();
        Self {
            slots,
            clock: 0,
            expression: 0.5,
        }
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        self.clock += 1;
        // Prefer a free slot; otherwise steal the oldest.
        let idx = self
            .slots
            .iter()
            .position(|s| s.note.is_none())
            .unwrap_or_else(|| {
                let mut best = 0;
                let mut best_age = u64::MAX;
                for (i, s) in self.slots.iter().enumerate() {
                    if s.age < best_age {
                        best_age = s.age;
                        best = i;
                    }
                }
                best
            });
        let freq = midi_to_hz(note);
        let expr = self.expression;
        let slot = &mut self.slots[idx];
        slot.note = Some(note);
        slot.age = self.clock;
        slot.engine.v_note_on(freq, velocity, expr);
    }

    fn note_off(&mut self, note: u8) {
        for slot in self.slots.iter_mut() {
            if slot.note == Some(note) {
                slot.engine.v_note_off();
                slot.note = None;
            }
        }
    }

    fn set_brightness(&mut self, amount: f32) {
        for slot in self.slots.iter_mut() {
            slot.engine.v_set_brightness(amount);
        }
    }

    fn set_vibrato(&mut self, rate_hz: f32, depth: f32) {
        for slot in self.slots.iter_mut() {
            slot.engine.v_set_vibrato(rate_hz, depth);
        }
    }

    /// Sum every slot to a stereo pair, with a small per-slot pan for width.
    fn process(&mut self) -> (f32, f32) {
        let n = self.slots.len();
        let mut l = 0.0;
        let mut r = 0.0;
        for (i, slot) in self.slots.iter_mut().enumerate() {
            let s = slot.engine.v_process();
            let pan = if n > 1 {
                (i as f32 / (n - 1) as f32 - 0.5) * 0.3
            } else {
                0.0
            };
            l += s * (1.0 - pan * 0.5);
            r += s * (1.0 + pan * 0.5);
        }
        (l, r)
    }
}

/// The active voice, rebuilt whenever the instrument selector, polyphony, or
/// sample rate changes. Per-family note handling:
///
/// * **Winds / Bowed** — monophonic cores wrapped in a [`PolyPool`]; note-off
///   releases the voice and frees its slot (release tail rings out).
/// * **Mallets** — already polyphonic; each note-on strikes, note-off is a
///   no-op (the bar rings out).
/// * **Plucked** — internal voice/course pools; note-on plucks/strikes,
///   note-off is a no-op.
/// * **Drums** — one engine re-struck per note-on; keyboard pitch selects the
///   stroke (low half -> deep stroke, high half -> sharp stroke); note-off is
///   a no-op.
/// * **Handpan / Tongue Drum** — scale-locked; note-on strikes the nearest
///   tone field, note-off optionally damps it.
enum AnyInstrument {
    Wind(PolyPool<Wind>),
    Bowed(PolyPool<Bowed>),
    Mallet(Mallet),
    Cifteli(Cifteli),
    Gayageum(Gayageum),
    Basitar(Basitar),
    Santur(Santur),
    Tar(Tar),
    Setar(Setar),
    Barbat(Barbat),
    Dohol(Dohol),
    Tombak(Tombak),
    Daf(Daf),
    Bodhran(Bodhran),
    Handpan {
        engine: Handpan,
        notes_midi: Vec<f32>,
        damp_on_release: bool,
    },
}

/// Index of the tone field (in `notes_midi`) nearest a MIDI note number.
fn nearest_field(notes_midi: &[f32], note: u8) -> Option<usize> {
    if notes_midi.is_empty() {
        return None;
    }
    let n = note as f32;
    let mut best = 0;
    let mut best_d = f32::INFINITY;
    for (i, &m) in notes_midi.iter().enumerate() {
        let d = (m - n).abs();
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    Some(best)
}

impl AnyInstrument {
    /// Build the selected voice at the given sample rate and polyphony.
    fn build(choice: InstrumentChoice, fs: f32, poly: usize) -> Self {
        use InstrumentChoice as C;
        let wind = |kind: WindKind| {
            AnyInstrument::Wind(PolyPool::new(poly, || Wind::new(fs, kind)))
        };
        let bowed = |kind: StringKind| {
            AnyInstrument::Bowed(PolyPool::new(poly, || Bowed::new(fs, kind)))
        };
        let mallet =
            |inst: MalletInstrument| AnyInstrument::Mallet(Mallet::new(fs, inst, poly.max(1)));
        match choice {
            C::Clarinet => wind(WindKind::Clarinet),
            C::Flute => wind(WindKind::Flute),
            C::Saxophone => wind(WindKind::Saxophone),
            C::Trumpet => wind(WindKind::Trumpet),
            C::Didgeridoo => wind(WindKind::Didgeridoo),
            C::Ney => wind(WindKind::Ney),
            C::Sorna => wind(WindKind::Sorna),
            C::TinWhistle => wind(WindKind::TinWhistle),
            C::IrishFlute => wind(WindKind::IrishFlute),
            C::UilleannPipes => wind(WindKind::UilleannPipes),

            C::Violin => bowed(StringKind::Violin),
            C::Viola => bowed(StringKind::Viola),
            C::Cello => bowed(StringKind::Cello),
            C::Bass => bowed(StringKind::Bass),
            C::Kamancheh => bowed(StringKind::Kamancheh),
            C::Fiddle => bowed(StringKind::Fiddle),

            C::Marimba => mallet(MalletInstrument::Marimba),
            C::Xylophone => mallet(MalletInstrument::Xylophone),
            C::Vibraphone => mallet(MalletInstrument::Vibraphone),
            C::Glockenspiel => mallet(MalletInstrument::Glockenspiel),
            C::TubularBell => mallet(MalletInstrument::TubularBell),
            C::ChurchBell => mallet(MalletInstrument::ChurchBell),
            C::MusicBox => mallet(MalletInstrument::MusicBox),
            C::SingingBowl => mallet(MalletInstrument::SingingBowl),

            C::Cifteli => AnyInstrument::Cifteli(Cifteli::new(fs)),
            C::Gayageum => AnyInstrument::Gayageum(Gayageum::with_voices(fs, poly.max(1))),
            C::Basitar => AnyInstrument::Basitar(Basitar::new(fs)),
            C::Santur => AnyInstrument::Santur(Santur::with_courses(fs, poly.max(1))),
            C::Tar => AnyInstrument::Tar(Tar::with_courses(fs, poly.max(1))),
            C::Setar => AnyInstrument::Setar(Setar::with_voices(fs, poly.max(1))),
            C::Barbat => AnyInstrument::Barbat(Barbat::with_courses(fs, poly.max(1))),

            C::Dohol => AnyInstrument::Dohol(Dohol::new(fs)),
            C::Tombak => AnyInstrument::Tombak(Tombak::new(fs)),
            C::Daf => AnyInstrument::Daf(Daf::new(fs)),
            C::Bodhran => AnyInstrument::Bodhran(Bodhran::new(fs)),

            C::Handpan | C::TongueDrum => {
                let scale = Scale::DKurd9;
                let build = if matches!(choice, C::TongueDrum) {
                    Build::TongueDrum
                } else {
                    Build::Handpan
                };
                let profile = VoiceProfile::preset(build, handpan_core::Size::Standard);
                let engine = Handpan::with_profile(fs, &scale.freqs(), &profile);
                AnyInstrument::Handpan {
                    engine,
                    notes_midi: scale.midi(),
                    damp_on_release: false,
                }
            }
        }
    }

    /// Push per-block continuous parameters into the engines. Values are read
    /// once per block (not sample-accurate) to keep dispatch simple.
    fn apply_params(&mut self, brightness: f32, expression: f32, motion: f32, sustain: f32) {
        match self {
            AnyInstrument::Wind(pool) => {
                pool.expression = expression;
                pool.set_brightness(brightness);
                pool.set_vibrato(5.0, motion * 0.3);
            }
            AnyInstrument::Bowed(pool) => {
                pool.expression = expression;
                pool.set_brightness(brightness);
                pool.set_vibrato(5.0, motion * 0.3);
            }
            AnyInstrument::Mallet(m) => {
                m.set_tremolo(5.0, motion);
            }
            AnyInstrument::Cifteli(v) => {
                v.set_brightness(brightness);
                v.set_sustain(sustain);
            }
            AnyInstrument::Gayageum(v) => {
                v.set_brightness(brightness);
                v.set_sustain(sustain);
                v.set_vibrato(5.0, motion * 50.0);
            }
            AnyInstrument::Basitar(v) => {
                v.set_brightness(brightness);
                v.set_sustain(sustain);
            }
            AnyInstrument::Santur(v) => {
                v.set_brightness(brightness);
                v.set_sustain(sustain);
            }
            AnyInstrument::Tar(v) => {
                v.set_brightness(brightness);
                v.set_sustain(sustain);
            }
            AnyInstrument::Setar(v) => {
                v.set_brightness(brightness);
                v.set_sustain(sustain);
            }
            AnyInstrument::Barbat(v) => {
                v.set_brightness(brightness);
                v.set_sustain(sustain);
            }
            // Drums and handpan expose no matching continuous controls here.
            _ => {}
        }
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        let hz = midi_to_hz(note);
        // Split point for drum stroke selection (middle C).
        let low = note < 60;
        match self {
            AnyInstrument::Wind(pool) => pool.note_on(note, velocity),
            AnyInstrument::Bowed(pool) => pool.note_on(note, velocity),
            AnyInstrument::Mallet(m) => m.strike_hz(hz, velocity),
            AnyInstrument::Cifteli(v) => v.pluck_melody(hz, velocity),
            AnyInstrument::Gayageum(v) => v.pluck(hz, velocity),
            // `pluck` strums a power chord; `pluck_low` sounds the single note.
            AnyInstrument::Basitar(v) => v.pluck_low(hz, velocity),
            AnyInstrument::Santur(v) => v.strike(hz, velocity),
            AnyInstrument::Tar(v) => v.pluck(hz, velocity),
            AnyInstrument::Setar(v) => v.pluck(hz, velocity),
            AnyInstrument::Barbat(v) => v.pluck(hz, velocity),
            AnyInstrument::Dohol(d) => {
                // Low keys -> deep central hit (pos 0); high keys -> rim (pos 1).
                d.strike(velocity, if low { 0.1 } else { 0.9 });
            }
            AnyInstrument::Tombak(d) => {
                if low {
                    d.tom(velocity);
                } else {
                    d.bak(velocity);
                }
            }
            AnyInstrument::Daf(d) => {
                if low {
                    d.dum(velocity);
                } else {
                    d.tak(velocity);
                }
            }
            AnyInstrument::Bodhran(d) => {
                if low {
                    d.low(velocity);
                } else {
                    d.high(velocity);
                }
            }
            AnyInstrument::Handpan {
                engine, notes_midi, ..
            } => {
                if let Some(field) = nearest_field(notes_midi, note) {
                    engine.strike(field, velocity);
                }
            }
        }
    }

    fn note_off(&mut self, note: u8) {
        match self {
            AnyInstrument::Wind(pool) => pool.note_off(note),
            AnyInstrument::Bowed(pool) => pool.note_off(note),
            AnyInstrument::Handpan {
                engine,
                notes_midi,
                damp_on_release,
            } => {
                if *damp_on_release {
                    if let Some(field) = nearest_field(notes_midi, note) {
                        engine.damp(field);
                    }
                }
            }
            // Struck / plucked voices ring out; note-off is a no-op.
            _ => {}
        }
    }

    fn process(&mut self) -> (f32, f32) {
        match self {
            AnyInstrument::Wind(pool) => pool.process(),
            AnyInstrument::Bowed(pool) => pool.process(),
            AnyInstrument::Mallet(m) => m.process(),
            AnyInstrument::Cifteli(v) => v.process(),
            AnyInstrument::Gayageum(v) => v.process(),
            AnyInstrument::Basitar(v) => v.process(),
            AnyInstrument::Santur(v) => v.process(),
            AnyInstrument::Tar(v) => v.process(),
            AnyInstrument::Setar(v) => v.process(),
            AnyInstrument::Barbat(v) => v.process(),
            AnyInstrument::Dohol(d) => d.process(),
            AnyInstrument::Tombak(d) => d.process(),
            AnyInstrument::Daf(d) => d.process(),
            AnyInstrument::Bodhran(d) => d.process(),
            AnyInstrument::Handpan { engine, .. } => engine.process(),
        }
    }
}

#[derive(Params)]
struct PugetParams {
    #[id = "instrument"]
    instrument: EnumParam<InstrumentChoice>,
    #[id = "gain"]
    gain: FloatParam,
    #[id = "brightness"]
    brightness: FloatParam,
    #[id = "expression"]
    expression: FloatParam,
    #[id = "motion"]
    motion: FloatParam,
    #[id = "sustain"]
    sustain: FloatParam,
    #[id = "polyphony"]
    polyphony: IntParam,
    #[id = "reverb_mix"]
    reverb_mix: FloatParam,
    #[id = "reverb_size"]
    reverb_size: FloatParam,
}

impl Default for PugetParams {
    fn default() -> Self {
        Self {
            instrument: EnumParam::new("Instrument", InstrumentChoice::Marimba),
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(-6.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-40.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::gain_skew_factor(-40.0, 6.0),
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db())
            .with_smoother(SmoothingStyle::Logarithmic(15.0)),
            brightness: FloatParam::new("Brightness", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0)),
            expression: FloatParam::new("Expression", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0)),
            motion: FloatParam::new("Motion", 0.2, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0)),
            sustain: FloatParam::new("Sustain", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0)),
            polyphony: IntParam::new("Polyphony", 8, IntRange::Linear { min: 1, max: 16 }),
            reverb_mix: FloatParam::new("Reverb Mix", 0.15, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0)),
            reverb_size: FloatParam::new("Reverb Size", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(30.0)),
        }
    }
}

struct PugetPlugin {
    params: Arc<PugetParams>,
    instr: Option<AnyInstrument>,
    reverb: Option<Reverb>,
    sample_rate: f32,
    /// Settings the current engine was built with, to detect rebuilds.
    built_instrument: InstrumentChoice,
    built_polyphony: i32,
}

impl Default for PugetPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(PugetParams::default()),
            instr: None,
            reverb: None,
            sample_rate: 48_000.0,
            built_instrument: InstrumentChoice::Marimba,
            built_polyphony: 8,
        }
    }
}

impl PugetPlugin {
    /// (Re)build the active voice from the current instrument / polyphony /
    /// sample-rate settings.
    fn rebuild(&mut self) {
        let choice = self.params.instrument.value();
        let poly = self.params.polyphony.value();
        self.instr = Some(AnyInstrument::build(choice, self.sample_rate, poly as usize));
        self.built_instrument = choice;
        self.built_polyphony = poly;
    }
}

impl Plugin for PugetPlugin {
    const NAME: &'static str = "Puget Studio";
    const VENDOR: &'static str = "Puget Audio";
    const URL: &'static str = "https://puget.audio";
    const EMAIL: &'static str = "hello@puget.audio";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.reverb = Some(Reverb::new(self.sample_rate));
        self.rebuild();
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Rebuild the voice if the instrument or polyphony changed.
        if self.instr.is_none()
            || self.params.instrument.value() != self.built_instrument
            || self.params.polyphony.value() != self.built_polyphony
        {
            self.rebuild();
        }

        // Move the engine out so the event loop can freely touch `self.params`.
        let mut instr = self.instr.take().unwrap();
        let mut reverb = self.reverb.take().unwrap();

        // Push per-block continuous params into the engines.
        instr.apply_params(
            self.params.brightness.value(),
            self.params.expression.value(),
            self.params.motion.value(),
            self.params.sustain.value(),
        );
        reverb.set_size(self.params.reverb_size.value());
        reverb.set_mix(self.params.reverb_mix.value());

        let mut next_event = context.next_event();
        for (sample_id, mut channels) in buffer.iter_samples().enumerate() {
            while let Some(event) = next_event {
                if event.timing() as usize != sample_id {
                    break;
                }
                match event {
                    NoteEvent::NoteOn { note, velocity, .. } => {
                        instr.note_on(note, velocity);
                    }
                    NoteEvent::NoteOff { note, .. } => {
                        instr.note_off(note);
                    }
                    _ => {}
                }
                next_event = context.next_event();
            }

            let (mut l, mut r) = instr.process();
            let (rl, rr) = reverb.process(l, r);
            l = rl;
            r = rr;

            let g = self.params.gain.smoothed.next();
            let mut it = channels.iter_mut();
            if let Some(cl) = it.next() {
                *cl = l * g;
            }
            if let Some(cr) = it.next() {
                *cr = r * g;
            }
        }

        self.instr = Some(instr);
        self.reverb = Some(reverb);
        ProcessStatus::Normal
    }
}

impl ClapPlugin for PugetPlugin {
    const CLAP_ID: &'static str = "audio.puget.studio";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Universal physical-modeling instrument — every Puget voice, one plugin");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for PugetPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"PugetStudio00001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(PugetPlugin);
nih_export_vst3!(PugetPlugin);
