RACK_DIR ?= ./.vcv-sdk/Rack-SDK
LIBDIR := libvcv_handpan
.PHONY: all plugin rustlib dist install clean
all: plugin
rustlib: $(LIBDIR)/libvcv_handpan.a $(LIBDIR)/lib.rs.h
$(LIBDIR)/libvcv_handpan.a $(LIBDIR)/lib.rs.h: $(LIBDIR)/src/lib.rs $(LIBDIR)/Cargo.toml $(LIBDIR)/build.rs
	$(MAKE) -C $(LIBDIR) libvcv_handpan.a lib.rs.h
plugin: rustlib res/Handpan.svg src/plugin.cpp src/Handpan.cpp
	$(MAKE) -f Makefile.racksdk
dist: plugin
	$(MAKE) -f Makefile.racksdk dist
install: dist
	$(MAKE) -f Makefile.racksdk install
clean:
	$(MAKE) -C $(LIBDIR) clean
	$(MAKE) -f Makefile.racksdk clean
