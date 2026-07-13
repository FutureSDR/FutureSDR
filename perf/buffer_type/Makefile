SHELL=/bin/bash

SHIELD=sudo systemd-run --uid=1000 --slice=sdr --wait -P -d

FSRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/fs_{0}_4_{1}_{2}_{3}_{4}_.csv".format(*x) for x in itertools.product(range(20), np.arange(1,26,2), [200000000], [128], ["smoln", "flow", "smoln-spsc", "flow-spsc", "local-smol-spsc", "local-flow-spsc", "local-smol-slab", "local-flow-slab", "smoln-slab", "flow-slab"])]))')

.PHONY: setup all

all: setup $(FSRESULTS)

setup:
	@echo "### SETTING UP"
	../perf_setup.sh

perf-data/fs%.csv: buffer_type.rs Cargo.toml
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval SAMPLES=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	$(eval CHUNK=$(shell python3 -c "print(\"$@\".split(\"_\")[5])"))
	$(eval CONFIG=$(shell python3 -c "print(\"$@\".split(\"_\")[6])"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo SAMPLES=$(SAMPLES)
	@echo CHUNK=$(CHUNK)
	@echo CONFIG=$(CONFIG)

	$(SHIELD) -- cargo run --release -- --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --chunk=$(CHUNK) --config=$(CONFIG) > $@
