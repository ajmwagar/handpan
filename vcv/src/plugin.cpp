// Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.
// Puget Handpan VCV Rack plugin — entry point.
#include "plugin.hpp"

Plugin *pluginInstance;

void init(Plugin *p) {
    pluginInstance = p;
    p->addModel(modelHandpan);
}
