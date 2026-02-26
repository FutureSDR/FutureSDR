SHELL=/bin/bash

RUNS=0 1 2 3 4
SIZES=10 100 1000
CONFIGS=normal opti

GRRESULTS=$(foreach r,$(RUNS),$(foreach s,$(SIZES),perf-data/gr_$(r)_$(s).csv))
FSRESULTS=$(foreach r,$(RUNS),$(foreach s,$(SIZES),$(foreach c,$(CONFIGS),perf-data/fs_$(r)_$(s)_$(c).csv)))

SHIELD=sudo systemd-run --uid=1000 --slice=sdr --wait -P -p AllowedCPUs=2,3,4,5 -d
PY_USER_SITE=$(shell python3 -c "import site; print(site.getusersitepackages())")
PY_SITE_PKGS=$(shell python3 -c "import site; print(':'.join(site.getsitepackages()))")
SHIELD_ENV=--setenv=HOME=$(HOME) --setenv=PATH=$(PATH) --setenv=PYTHONPATH=$(PY_USER_SITE):$(PY_SITE_PKGS):$(PYTHONPATH) --setenv=LD_LIBRARY_PATH=$(LD_LIBRARY_PATH)

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
	$(eval SIZE=$(word 2,$(subst _, ,$*)))
	@echo "GR run=$(RUN) size=$(SIZE)"
	$(SHIELD) $(SHIELD_ENV) -- python3 wifi_rx.py --run=$(RUN) --file=wlan-$(SIZE).cf32 > $@

perf-data/fs_%.csv:
	mkdir -p perf-data
	$(eval RUN=$(word 1,$(subst _, ,$*)))
	$(eval SIZE=$(word 2,$(subst _, ,$*)))
	$(eval CONFIG=$(word 3,$(subst _, ,$*)))
	@echo "FS run=$(RUN) size=$(SIZE) config=$(CONFIG)"
	$(SHIELD) -- cargo run --release --bin rx -- --run=$(RUN) --file=wlan-$(SIZE).cf32 --config=$(CONFIG) > $@

parse:
	./parse.sh

plot: parse
	./plot.py

clean:
	rm -rf perf-data wlan.pdf
