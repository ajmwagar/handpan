# =============================================================================
# Puget Handpan — Alchemy Lab firmware. The DSP is handpan-core, cross-compiled
# from Rust (dsp/ -> libhandpan_dsp.a) and linked into the C++ Alchemy SDK.
#
#   git submodule update --init   # fetch lib/alchemy-sdk + lib/libDaisy first
#   make libdaisy                 # build libDaisy once
#   make                          # build the Rust DSP lib + firmware (BOARD=v2)
#   make program-dfu              # flash over USB (module in DFU mode)
#   make clean
#
# Requires: rustup target add thumbv7em-none-eabihf, and an arm-none-eabi
# toolchain (for the C++/libDaisy side).
# =============================================================================

TARGET = pedal

# Alchemy Lab board revision: v1 | v2
BOARD ?= v2
ifeq ($(filter $(BOARD),v1 v2),)
$(error BOARD must be 'v1' or 'v2' (got '$(BOARD)'))
endif

ALCHEMY_DIR  = lib/alchemy-sdk
LIBDAISY_DIR = lib/libDaisy

# ── Rust DSP static library (handpan-core) ──────────────────────────────────
RUST_DIR     = dsp
RUST_TARGET  = thumbv7em-none-eabihf
RUST_LIB_DIR = $(RUST_DIR)/target/$(RUST_TARGET)/release
RUST_LIB     = $(RUST_LIB_DIR)/libhandpan_dsp.a

# ── App sources ─────────────────────────────────────────────────────────────
CPP_SOURCES = src/pedal.cpp

# ── Alchemy SDK, compiled from the submodule ────────────────────────────────
CPP_SOURCES += $(sort $(shell find $(ALCHEMY_DIR)/framework/src -name '*.cpp'))
CPP_SOURCES += $(sort $(wildcard $(ALCHEMY_DIR)/hardware/alchemy-lab/$(BOARD)/src/*.cpp))

C_INCLUDES += \
    -Isrc \
    -I$(ALCHEMY_DIR)/framework/include \
    -I$(ALCHEMY_DIR)/hardware/include \
    -I$(ALCHEMY_DIR)/hardware/alchemy-lab/$(BOARD)/include

ifeq ($(BOARD),v2)
C_DEFS += -DALCHEMY_BOARD_V2
endif

# ── Daisy bootloader build (BOOT_SRAM) ──────────────────────────────────────
APP_TYPE = BOOT_SRAM
LDSCRIPT = $(ALCHEMY_DIR)/cmake/linkers/alchemy_stm32h750ib_sram.lds
CPP_STANDARD = -std=gnu++17

SYSTEM_FILES_DIR = $(LIBDAISY_DIR)/core
include $(SYSTEM_FILES_DIR)/Makefile

# ── Link the Rust DSP lib ───────────────────────────────────────────────────
LIBDIR += -L$(RUST_LIB_DIR)
LIBS   += -Wl,--start-group -lhandpan_dsp -lc -lm -lnosys -Wl,--end-group

.PHONY: rust-dsp
rust-dsp:
	cd $(RUST_DIR) && cargo build --release

$(RUST_LIB): rust-dsp
$(BUILD_DIR)/$(TARGET).elf: $(RUST_LIB)

BOARD_STAMP := $(BUILD_DIR)/.board-$(BOARD)
ifeq ($(wildcard $(BOARD_STAMP)),)
_BOARD_GUARD := $(shell rm -f $(BUILD_DIR)/*.o $(BUILD_DIR)/*.d $(BUILD_DIR)/*.lst $(BUILD_DIR)/.board-* 2>/dev/null; mkdir -p $(BUILD_DIR); touch $(BOARD_STAMP))
endif

.PHONY: libdaisy
libdaisy:
	$(MAKE) -C $(LIBDAISY_DIR)

.PHONY: rust-clean
clean: rust-clean
rust-clean:
	cd $(RUST_DIR) && cargo clean
