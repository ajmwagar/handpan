// Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.
// Puget Handpan VCV Rack plugin — shared declarations.
#pragma once
#include <rack.hpp>

// Generated cxx bridge header (Rust FFI to handpan-core).
#include "lib.rs.h"

using namespace rack;

extern Plugin *pluginInstance;
extern Model *modelHandpan;
