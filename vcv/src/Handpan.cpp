// Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.
//
// Puget Handpan — VCV Rack module. The whole instrument runs in Rust
// (handpan-core); this is the thin wrapper that maps VCV I/O to the engine.
//
// Panel mirrors the Electrosmith Patch.Init() so the same control scheme works
// in software and on that hardware:
//   4 knobs  → Scale, Size, Air, Damp
//   CV in    → V/Oct (quantized), Velocity
//   Gate in  → Strike, Gu
//   Audio out → L / R
// Extra VCV-only controls (Position, Artic, Shell, Play mode) live as params.
//
// Voltage convention: VCV audio is ±5 V; the voice outputs ~±1, so ×5 out.
// V/Oct is 1 V/oct, 0 V = the ding.

#include "plugin.hpp"

struct Handpan : Module {
    enum ParamId {
        SCALE_PARAM,
        SIZE_PARAM,
        AIR_PARAM,
        DAMP_PARAM,
        BUILD_PARAM,
        POSITION_PARAM,
        ARTIC_PARAM,
        SHELL_PARAM,
        PLAYMODE_PARAM,
        PARAMS_LEN
    };
    enum InputId {
        VOCT_INPUT,
        VELOCITY_INPUT,
        STRIKE_INPUT,
        GU_INPUT,
        CLOCK_INPUT,
        INPUTS_LEN
    };
    enum OutputId { LEFT_OUTPUT, RIGHT_OUTPUT, OUTPUTS_LEN };
    enum LightId { STRIKE_LIGHT, LIGHTS_LEN };

    rust::Box<handpan::HandpanEngine> engine;
    dsp::SchmittTrigger strikeTrig, guTrig, clockTrig;

    // Cached build config to avoid rebuilding every sample.
    int lastScale = -1, lastSize = -1, lastBuild = -1;
    float lightPhase = 0.f;

    Handpan()
        : engine(handpan::new_handpan_engine(APP->engine->getSampleRate(), 3, 2, 0)) {
        config(PARAMS_LEN, INPUTS_LEN, OUTPUTS_LEN, LIGHTS_LEN);
        int scaleMax = (int)handpan::scale_count() - 1;
        configParam(SCALE_PARAM, 0.f, (float)scaleMax, 3.f, "Scale");
        paramQuantities[SCALE_PARAM]->snapEnabled = true;
        configParam(SIZE_PARAM, 0.f, 3.f, 2.f, "Size");
        paramQuantities[SIZE_PARAM]->snapEnabled = true;
        configParam(AIR_PARAM, 0.f, 0.6f, 0.16f, "Air");
        configParam(DAMP_PARAM, 0.f, 1.f, 0.f, "Damp");
        configSwitch(BUILD_PARAM, 0.f, 1.f, 0.f, "Build", {"Handpan", "Tongue Drum"});
        configParam(POSITION_PARAM, 0.f, 1.f, 0.4f, "Strike position (center→edge)");
        configSwitch(ARTIC_PARAM, 0.f, 2.f, 0.f, "Articulation", {"Open", "Mute", "Slap"});
        paramQuantities[ARTIC_PARAM]->snapEnabled = true;
        configParam(SHELL_PARAM, 0.f, 0.3f, 0.05f, "Shell interaction");
        configSwitch(PLAYMODE_PARAM, 0.f, 6.f, 0.f, "Internal play mode",
                     {"Manual", "Up", "Down", "UpDown", "Random", "Wander", "Euclid"});
        paramQuantities[PLAYMODE_PARAM]->snapEnabled = true;

        configInput(VOCT_INPUT, "V/Oct (quantized)");
        configInput(VELOCITY_INPUT, "Velocity");
        configInput(STRIKE_INPUT, "Strike");
        configInput(GU_INPUT, "Gu (bass)");
        configInput(CLOCK_INPUT, "Clock (internal player)");
        configOutput(LEFT_OUTPUT, "Left");
        configOutput(RIGHT_OUTPUT, "Right");
    }

    void onSampleRateChange(const SampleRateChangeEvent &e) override {
        handpan::reconfigure(*engine, e.sampleRate, (uint32_t)lastScale,
                             (uint32_t)lastBuild, (uint32_t)lastSize);
    }

    void process(const ProcessArgs &args) override {
        int scale = (int)std::round(params[SCALE_PARAM].getValue());
        int size = (int)std::round(params[SIZE_PARAM].getValue());
        int build = (int)std::round(params[BUILD_PARAM].getValue());
        if (scale != lastScale || size != lastSize || build != lastBuild) {
            handpan::reconfigure(*engine, args.sampleRate, (uint32_t)scale,
                                 (uint32_t)build, (uint32_t)size);
            lastScale = scale;
            lastSize = size;
            lastBuild = build;
        }

        // Live parameters.
        handpan::set_air(*engine, params[AIR_PARAM].getValue());
        handpan::set_damp(*engine, params[DAMP_PARAM].getValue());
        handpan::set_shell(*engine, params[SHELL_PARAM].getValue());
        handpan::set_play_mode(*engine, (uint32_t)std::round(params[PLAYMODE_PARAM].getValue()));

        float vel = inputs[VELOCITY_INPUT].isConnected()
                        ? clamp(inputs[VELOCITY_INPUT].getVoltage() / 10.f, 0.05f, 1.f)
                        : 0.85f;
        float volts = inputs[VOCT_INPUT].getVoltage(); // 0 V = ding
        float pos = params[POSITION_PARAM].getValue();
        uint32_t artic = (uint32_t)std::round(params[ARTIC_PARAM].getValue());

        if (strikeTrig.process(inputs[STRIKE_INPUT].getVoltage(), 0.1f, 1.f)) {
            handpan::strike(*engine, volts, vel, artic, pos);
            lightPhase = 1.f;
        }
        if (guTrig.process(inputs[GU_INPUT].getVoltage(), 0.1f, 1.f)) {
            handpan::strike_gu(*engine, vel);
        }
        // Clock drives the internal generative player (normalled "plays itself").
        if (clockTrig.process(inputs[CLOCK_INPUT].getVoltage(), 0.1f, 1.f)) {
            handpan::clock(*engine, vel);
            lightPhase = 1.f;
        }

        handpan::StereoFrame f = handpan::process(*engine);
        outputs[LEFT_OUTPUT].setVoltage(f.l * 5.f);
        outputs[RIGHT_OUTPUT].setVoltage(f.r * 5.f);

        lightPhase = std::max(0.f, lightPhase - args.sampleTime * 6.f);
        lights[STRIKE_LIGHT].setBrightness(lightPhase);
    }
};

struct HandpanWidget : ModuleWidget {
    HandpanWidget(Handpan *module) {
        setModule(module);
        setPanel(createPanel(asset::plugin(pluginInstance, "res/Handpan.svg")));

        addChild(createWidget<ScrewSilver>(Vec(RACK_GRID_WIDTH, 0)));
        addChild(createWidget<ScrewSilver>(
            Vec(box.size.x - 2 * RACK_GRID_WIDTH, RACK_GRID_HEIGHT - RACK_GRID_WIDTH)));

        // Knob row 1: Scale, Size.
        addParam(createParamCentered<RoundBlackKnob>(mm2px(Vec(15, 22)), module, Handpan::SCALE_PARAM));
        addParam(createParamCentered<RoundBlackKnob>(mm2px(Vec(45, 22)), module, Handpan::SIZE_PARAM));
        // Knob row 2: Air, Damp.
        addParam(createParamCentered<RoundBlackKnob>(mm2px(Vec(15, 42)), module, Handpan::AIR_PARAM));
        addParam(createParamCentered<RoundBlackKnob>(mm2px(Vec(45, 42)), module, Handpan::DAMP_PARAM));
        // Row 3: Position, Shell, Artic, Build.
        addParam(createParamCentered<Trimpot>(mm2px(Vec(12, 60)), module, Handpan::POSITION_PARAM));
        addParam(createParamCentered<Trimpot>(mm2px(Vec(27, 60)), module, Handpan::SHELL_PARAM));
        addParam(createParamCentered<CKSSThree>(mm2px(Vec(42, 60)), module, Handpan::ARTIC_PARAM));
        addParam(createParamCentered<CKSS>(mm2px(Vec(54, 60)), module, Handpan::BUILD_PARAM));
        // Play mode.
        addParam(createParamCentered<Trimpot>(mm2px(Vec(12, 74)), module, Handpan::PLAYMODE_PARAM));

        // Inputs.
        addInput(createInputCentered<PJ301MPort>(mm2px(Vec(10, 92)), module, Handpan::VOCT_INPUT));
        addInput(createInputCentered<PJ301MPort>(mm2px(Vec(25, 92)), module, Handpan::VELOCITY_INPUT));
        addInput(createInputCentered<PJ301MPort>(mm2px(Vec(40, 92)), module, Handpan::STRIKE_INPUT));
        addInput(createInputCentered<PJ301MPort>(mm2px(Vec(10, 106)), module, Handpan::GU_INPUT));
        addInput(createInputCentered<PJ301MPort>(mm2px(Vec(25, 106)), module, Handpan::CLOCK_INPUT));

        // Outputs.
        addOutput(createOutputCentered<PJ301MPort>(mm2px(Vec(40, 106)), module, Handpan::LEFT_OUTPUT));
        addOutput(createOutputCentered<PJ301MPort>(mm2px(Vec(52, 106)), module, Handpan::RIGHT_OUTPUT));

        addChild(createLightCentered<SmallLight<GreenLight>>(mm2px(Vec(52, 22)), module, Handpan::STRIKE_LIGHT));
    }
};

Model *modelHandpan = createModel<Handpan, HandpanWidget>("Handpan");
