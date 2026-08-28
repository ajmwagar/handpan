//! Puget Handpan — a VST3/CLAP instrument that wraps the `handpan-core` modal
//! voice. MIDI note-ons strike the matching tone field; there is no sampled
//! content and no GUI yet (parameters only).
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

use std::sync::Arc;

use handpan_core::{scale::Scale, Build, Handpan, Size, VoiceProfile};
use nih_plug::prelude::*;

/// Named tunings exposed as a host parameter.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
enum ScaleChoice {
    #[id = "d_kurd_9"]
    #[name = "D Kurd 9"]
    DKurd9,
    #[id = "d_celtic_minor_9"]
    #[name = "D Celtic Minor 9"]
    DCelticMinor9,
    #[id = "csh_kurd_9"]
    #[name = "C# Kurd 9"]
    CshKurd9,
    #[id = "d_hijaz_9"]
    #[name = "D Hijaz 9"]
    DHijaz9,
    #[id = "d_minor_pentatonic_9"]
    #[name = "D Minor Pentatonic 9"]
    DMinorPentatonic9,
    #[id = "d_major_9"]
    #[name = "D Major 9"]
    DMajor9,
    #[id = "d_hijaz_kar_9"]
    #[name = "D Hijaz Kar 9"]
    DHijazKar9,
    #[id = "d_insen_9"]
    #[name = "D Insen 9"]
    DInsen9,
}

impl ScaleChoice {
    fn to_scale(self) -> Scale {
        match self {
            ScaleChoice::DKurd9 => Scale::DKurd9,
            ScaleChoice::DCelticMinor9 => Scale::DCelticMinor9,
            ScaleChoice::CshKurd9 => Scale::CshKurd9,
            ScaleChoice::DHijaz9 => Scale::DHijaz9,
            ScaleChoice::DMinorPentatonic9 => Scale::DMinorPentatonic9,
            ScaleChoice::DMajor9 => Scale::DMajor9,
            ScaleChoice::DHijazKar9 => Scale::DHijazKar9,
            ScaleChoice::DInsen9 => Scale::DInsen9,
        }
    }
}

/// Instrument build: dimpled handpan vs. cut steel tongue drum.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
enum BuildChoice {
    #[id = "handpan"]
    #[name = "Handpan (dimpled)"]
    Handpan,
    #[id = "tongue_drum"]
    #[name = "Tongue Drum (cut)"]
    TongueDrum,
}

impl BuildChoice {
    fn to_build(self) -> Build {
        match self {
            BuildChoice::Handpan => Build::Handpan,
            BuildChoice::TongueDrum => Build::TongueDrum,
        }
    }
}

/// Instrument size / tongue size.
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
enum SizeChoice {
    #[id = "small"]
    #[name = "Small"]
    Small,
    #[id = "standard"]
    #[name = "Standard"]
    Standard,
    #[id = "large"]
    #[name = "Large"]
    Large,
    #[id = "bass"]
    #[name = "Bass"]
    Bass,
}

impl SizeChoice {
    fn to_size(self) -> Size {
        match self {
            SizeChoice::Small => Size::Small,
            SizeChoice::Standard => Size::Standard,
            SizeChoice::Large => Size::Large,
            SizeChoice::Bass => Size::Bass,
        }
    }
}

#[derive(Params)]
struct HandpanParams {
    #[id = "gain"]
    gain: FloatParam,
    #[id = "coupling"]
    coupling: FloatParam,
    #[id = "sustain"]
    sustain: FloatParam,
    #[id = "body"]
    body: FloatParam,
    #[id = "scale"]
    scale: EnumParam<ScaleChoice>,
    #[id = "air"]
    air: FloatParam,
    #[id = "build"]
    build: EnumParam<BuildChoice>,
    #[id = "size"]
    size: EnumParam<SizeChoice>,
    #[id = "damp_on_release"]
    damp_on_release: BoolParam,
}

impl Default for HandpanParams {
    fn default() -> Self {
        Self {
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
            coupling: FloatParam::new("Halo", 0.06, FloatRange::Linear { min: 0.0, max: 0.4 })
                .with_smoother(SmoothingStyle::Linear(20.0)),
            sustain: FloatParam::new("Sustain", 1.0, FloatRange::Linear { min: 0.3, max: 2.0 }),
            body: FloatParam::new("Body", 0.12, FloatRange::Linear { min: 0.0, max: 0.5 }),
            air: FloatParam::new("Air", 0.16, FloatRange::Linear { min: 0.0, max: 0.6 })
                .with_smoother(SmoothingStyle::Linear(30.0)),
            scale: EnumParam::new("Scale", ScaleChoice::DKurd9),
            build: EnumParam::new("Build", BuildChoice::Handpan),
            size: EnumParam::new("Size", SizeChoice::Standard),
            damp_on_release: BoolParam::new("Damp on Release", false),
        }
    }
}

struct HandpanPlugin {
    params: Arc<HandpanParams>,
    engine: Option<Handpan>,
    sample_rate: f32,
    /// MIDI note numbers of the current tone fields, ding first.
    notes_midi: Vec<f32>,
    /// Settings the current engine was built with, to detect rebuilds.
    built_scale: ScaleChoice,
    built_build: BuildChoice,
    built_size: SizeChoice,
    built_sustain: f32,
    built_body: f32,
}

impl Default for HandpanPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(HandpanParams::default()),
            engine: None,
            sample_rate: 48_000.0,
            notes_midi: Vec::new(),
            built_scale: ScaleChoice::DKurd9,
            built_build: BuildChoice::Handpan,
            built_size: SizeChoice::Standard,
            built_sustain: f32::NAN,
            built_body: f32::NAN,
        }
    }
}

impl HandpanPlugin {
    /// Rebuild the voice from the current build/size/scale/sustain/body params.
    fn rebuild(&mut self) {
        let scale_c = self.params.scale.value();
        let build_c = self.params.build.value();
        let size_c = self.params.size.value();
        let scale = scale_c.to_scale();
        let sustain = self.params.sustain.value();
        let body = self.params.body.value();

        // Start from the named preset, then apply the live tone knobs.
        let mut profile = VoiceProfile::preset(build_c.to_build(), size_c.to_size());
        profile.decay_scale *= sustain;
        profile.body = body;
        profile.coupling = self.params.coupling.value();

        self.notes_midi = scale.midi();
        self.engine = Some(Handpan::with_profile(self.sample_rate, &scale.freqs(), &profile));
        self.built_scale = scale_c;
        self.built_build = build_c;
        self.built_size = size_c;
        self.built_sustain = sustain;
        self.built_body = body;
    }

    /// Index of the tone field nearest a MIDI note number.
    fn nearest_field(&self, note: u8) -> Option<usize> {
        if self.notes_midi.is_empty() {
            return None;
        }
        let n = note as f32;
        let mut best = 0;
        let mut best_d = f32::INFINITY;
        for (i, &m) in self.notes_midi.iter().enumerate() {
            let d = (m - n).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        Some(best)
    }
}

impl Plugin for HandpanPlugin {
    const NAME: &'static str = "Puget Handpan";
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
        self.rebuild();
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Rebuild if a bake-time parameter changed since the last block.
        if self.engine.is_none()
            || self.params.scale.value() != self.built_scale
            || self.params.build.value() != self.built_build
            || self.params.size.value() != self.built_size
            || (self.params.sustain.value() - self.built_sustain).abs() > 1e-6
            || (self.params.body.value() - self.built_body).abs() > 1e-6
        {
            self.rebuild();
        }
        // Move the engine out so the event loop can call `&self` helpers
        // (nearest_field, params) without aliasing a mutable borrow.
        let mut engine = self.engine.take().unwrap();
        engine.set_coupling(self.params.coupling.value());
        engine.set_air(self.params.air.value());
        let damp_on_release = self.params.damp_on_release.value();

        let mut next_event = context.next_event();
        for (sample_id, mut channels) in buffer.iter_samples().enumerate() {
            // Handle all events scheduled for this sample.
            while let Some(event) = next_event {
                if event.timing() as usize != sample_id {
                    break;
                }
                match event {
                    NoteEvent::NoteOn { note, velocity, .. } => {
                        if let Some(field) = self.nearest_field(note) {
                            engine.strike(field, velocity);
                        }
                    }
                    NoteEvent::NoteOff { note, .. } if damp_on_release => {
                        if let Some(field) = self.nearest_field(note) {
                            engine.damp(field);
                        }
                    }
                    _ => {}
                }
                next_event = context.next_event();
            }

            let (l, r) = engine.process();
            let g = self.params.gain.smoothed.next();
            let mut it = channels.iter_mut();
            if let Some(cl) = it.next() {
                *cl = l * g;
            }
            if let Some(cr) = it.next() {
                *cr = r * g;
            }
        }

        self.engine = Some(engine);
        ProcessStatus::Normal
    }
}

impl ClapPlugin for HandpanPlugin {
    const CLAP_ID: &'static str = "audio.puget.handpan";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Polyphonic handpan — modal physical-modeling instrument");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for HandpanPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"PugetHandpan0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_clap!(HandpanPlugin);
nih_export_vst3!(HandpanPlugin);
