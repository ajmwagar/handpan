RUST_TARGET ?= release
CARGO_PROFILE := --$(RUST_TARGET)
CRATE_ROOT := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))
CARGO_DIR  := $(CRATE_ROOT)target/$(RUST_TARGET)

.PHONY: all clean
all: libvcv_handpan.a lib.rs.h

libvcv_handpan.a: src/lib.rs Cargo.toml build.rs
	cargo build $(CARGO_PROFILE) --manifest-path "$(CRATE_ROOT)Cargo.toml"
	cp "$(CARGO_DIR)/libvcv_handpan.a" libvcv_handpan.a

lib.rs.h: libvcv_handpan.a
	cp "$(CRATE_ROOT)target/cxxbridge/vcv-handpan-lib/src/lib.rs.h" lib.rs.h

clean:
	cargo clean --manifest-path "$(CRATE_ROOT)Cargo.toml"
	rm -f libvcv_handpan.a lib.rs.h
