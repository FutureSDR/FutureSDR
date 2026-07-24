SHELL=/bin/bash

RUNS=0 1 2 3 4 5 6 7 8 9
PAYLOADS=16 64 128 255
CONFIGS=normal opti

GRRESULTS=$(foreach r,$(RUNS),$(foreach p,$(PAYLOADS),perf-data/gr_$(r)_$(p).csv))
FSRESULTS=$(foreach r,$(RUNS),$(foreach p,$(PAYLOADS),$(foreach c,$(CONFIGS),perf-data/fs_$(r)_$(p)_$(c).csv)))

SHIELD=sudo systemd-run --uid=1000 --slice=sdr --wait -P -d
LORA_PREFIX?=$(HOME)/usr

dump_file = $(firstword $(wildcard lora-dumps/samples_sf7_*_$(1)B_*.cf32))

.PHONY: setup all gr fs parse plot clean

all: setup gr fs parse plot

gr: setup $(GRRESULTS)
fs: setup $(FSRESULTS)

setup:
	@echo "### SETTING UP"
	../perf_setup.sh

build/gnuradio-rx: CMakeLists.txt gnuradio-rx.cc
	cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DCMAKE_PREFIX_PATH=$(LORA_PREFIX)
	cmake --build build -j

perf-data/gr_%.csv: build/gnuradio-rx
	mkdir -p perf-data
	$(eval RUN=$(word 1,$(subst _, ,$*)))
	$(eval PAYLOAD=$(word 2,$(subst _, ,$*)))
	$(eval FILE=$(call dump_file,$(PAYLOAD)))
	@test -f "$(FILE)" || (echo "missing LoRa dump for $(PAYLOAD)B" >&2; exit 1)
	@echo "GR run=$(RUN) payload=$(PAYLOAD) file=$(FILE)"
	$(SHIELD) -- ./build/gnuradio-rx --run=$(RUN) --file=$(FILE) > $@

perf-data/fs_%.csv: rx.rs Cargo.toml
	mkdir -p perf-data
	$(eval RUN=$(word 1,$(subst _, ,$*)))
	$(eval PAYLOAD=$(word 2,$(subst _, ,$*)))
	$(eval CONFIG=$(word 3,$(subst _, ,$*)))
	$(eval FILE=$(call dump_file,$(PAYLOAD)))
	@test -f "$(FILE)" || (echo "missing LoRa dump for $(PAYLOAD)B" >&2; exit 1)
	@echo "FS run=$(RUN) payload=$(PAYLOAD) config=$(CONFIG) file=$(FILE)"
	$(SHIELD) -- cargo run --release --bin rx -- --run=$(RUN) --file=$(FILE) --config=$(CONFIG) > $@

parse:
	./parse.sh

plot: parse
	./plot.py

clean:
	rm -rf build perf-data lora.pdf
