SHELL=/bin/bash

GRRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/gr_{0}_4_{1}_{2}_legacy_.csv".format(*x) for x in itertools.product(range(5), np.arange(1,30,2), [700000000])]))')
FSRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/fs_{0}_4_{1}_{2}_{3}_.csv".format(*x) for x in itertools.product(range(5), np.arange(1,30,2), [700000000], ["flow", "smoln", "slab"])]))')

SHIELD=sudo systemd-run --uid=1000 --slice=sdr --wait -P -p AllowedCPUs=2,3,4,5 -d

.PHONY: setup all clean perf_smol perf_flow perf_slab perf_gr

all: setup $(GRRESULTS) $(FSRESULTS)

gr: setup $(GRRESULTS)
fs: setup $(FSRESULTS)

setup:
	@echo "### SETTING UP"
	../migrate-processes.sh

perf-data/gr%.csv: build/null_flowgraph
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval SAMPLES=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	$(eval CONFIG=$(shell python3 -c "print(\"$@\".split(\"_\")[5])"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo SAMPLES=$(SAMPLES)
	@echo CONFIG=$(CONFIG)

	$(SHIELD) -- ./build/null_flowgraph --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) > $@

perf-data/fs%.csv:
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval SAMPLES=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	$(eval CONFIG=$(shell python3 -c "print(\"$@\".split(\"_\")[5])"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo SAMPLES=$(SAMPLES)
	@echo CONFIG=$(CONFIG)

	$(SHIELD) -- cargo run --release -- --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --config=$(CONFIG) > $@

clean:
	rm -rf build

PERF=perf stat -B -e context-switches,cpu-migrations,LLC-loads,LLC-load-misses,LLC-stores,LLC-store-misses,L1-dcache-load-misses,L1-dcache-loads,L1-dcache-stores,L1-icache-load-misses,cache-misses,cache-references,dTLB-load-misses,dTLB-loads,dTLB-store-misses,dTLB-stores,iTLB-load-misses,iTLB-loads
PERF_PARAMS=--max-copy=128 --samples 500000000 --pipes=6 --stages=10

perf_flow:
	cargo build --release --bin null
	$(SHIELD) -- $(PERF) cargo run --release --bin null -- $(PERF_PARAMS) --config flow

perf_smol:
	cargo build --release --bin null
	$(SHIELD) -- $(PERF) cargo run --release --bin null -- $(PERF_PARAMS) --config smoln

perf_slab:
	cargo build --release --bin null
	$(SHIELD) -- $(PERF) cargo run --release --bin null -- $(PERF_PARAMS) --config slab

perf_gr:
	$(SHIELD) -- $(PERF) ./build/null_flowgraph $(PERF_PARAMS)
