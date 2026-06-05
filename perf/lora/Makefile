SHELL=/bin/bash

RUNS=0 1 2 3 4
PAYLOADS=16 64 128 255
CONFIGS=normal opti

GRRESULTS=$(foreach r,$(RUNS),$(foreach p,$(PAYLOADS),perf-data/gr_$(r)_$(p).csv))
FSRESULTS=$(foreach r,$(RUNS),$(foreach p,$(PAYLOADS),$(foreach c,$(CONFIGS),perf-data/fs_$(r)_$(p)_$(c).csv)))

SHIELD=sudo systemd-run --uid=1000 --slice=sdr --wait -P -d
PY_USER_SITE=$(shell python3 -c "import site; print(site.getusersitepackages())")
PY_SITE_PKGS=$(shell python3 -c "import site; print(':'.join(site.getsitepackages()))")
SHIELD_ENV=--setenv=HOME=$(HOME) --setenv=PATH=$(PATH) --setenv=PYTHONPATH=$(PY_USER_SITE):$(PY_SITE_PKGS):$(PYTHONPATH) --setenv=LD_LIBRARY_PATH=$(LD_LIBRARY_PATH)

dump_file = $(firstword $(wildcard lora-dumps/*_$(1)B_*.cf32))

.PHONY: setup all gr fs parse plot clean

all: setup gr fs parse plot

gr: setup $(GRRESULTS)
fs: setup $(FSRESULTS)

setup:
	@echo "### SETTING UP"
	../migrate-processes.sh

perf-data/gr_%.csv:
	mkdir -p perf-data
	$(eval RUN=$(word 1,$(subst _, ,$*)))
	$(eval PAYLOAD=$(word 2,$(subst _, ,$*)))
	$(eval FILE=$(call dump_file,$(PAYLOAD)))
	@test -n "$(FILE)" || (echo "missing LoRa dump for $(PAYLOAD)B" >&2; exit 1)
	@echo "GR run=$(RUN) payload=$(PAYLOAD) file=$(FILE)"
	$(SHIELD) $(SHIELD_ENV) -- python3 gnuradio-rx.py --run=$(RUN) --file=$(FILE) > $@

perf-data/fs_%.csv:
	mkdir -p perf-data
	$(eval RUN=$(word 1,$(subst _, ,$*)))
	$(eval PAYLOAD=$(word 2,$(subst _, ,$*)))
	$(eval CONFIG=$(word 3,$(subst _, ,$*)))
	$(eval FILE=$(call dump_file,$(PAYLOAD)))
	@test -n "$(FILE)" || (echo "missing LoRa dump for $(PAYLOAD)B" >&2; exit 1)
	@echo "FS run=$(RUN) payload=$(PAYLOAD) config=$(CONFIG) file=$(FILE)"
	$(SHIELD) -- cargo run --release --bin rx -- --run=$(RUN) --file=$(FILE) --config=$(CONFIG) > $@

parse:
	./parse.sh

plot: parse
	./plot.py

clean:
	rm -rf perf-data lora.pdf
