SHELL=/bin/bash

GRRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/gr_{0}_6_{1}_{2}_{3}_legacy_.csv".format(*x) for x in itertools.product(range(50), np.arange(1,25,2), [200000000], [512])]))')
NRRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/nr_{0}_6_{1}_{2}_{3}_{4}_.csv".format(*x) for x in itertools.product(range(10), np.arange(1,25,2), [200000000], [512], ["smol1", "smoln", "flow"])]))')

.PHONY: setup all clean perf_smol perf_flow perf_gr

all: setup $(GRRESULTS) $(NRRESULTS)

gr: setup $(GRRESULTS)
nr: setup $(NRRESULTS)

setup:
	@echo "### SETTING UP"
	../create_cpuset.sh
	../cpugov.sh performance
	../irq_affinity.sh

perf-data/gr%.csv: build/null_rand_flowgraph
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval SAMPLES=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	$(eval MAX=$(shell python3 -c "print(\"$@\".split(\"_\")[5])"))
	$(eval SCHEDULER=$(shell python3 -c "print(\"$@\".split(\"_\")[6])"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo SAMPLES=$(SAMPLES)
	@echo MAX=$(MAX)
	@echo SCHEDULER=$(SCHEDULER)

	cset shield --userset=sdr --exec -- ./build/null_rand_flowgraph --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --max_copy=$(MAX) | grep -v cset > $@

perf-data/nr%.csv: null_rand.rs
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval SAMPLES=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	$(eval MAX=$(shell python3 -c "print(\"$@\".split(\"_\")[5])"))
	$(eval SCHEDULER=$(shell python3 -c "print(\"$@\".split(\"_\")[6])"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo SAMPLES=$(SAMPLES)
	@echo MAX=$(MAX)
	@echo SCHEDULER=$(SCHEDULER)

	cset shield --userset=sdr --exec -- cargo run --release -- --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --max_copy=$(MAX) --scheduler=$(SCHEDULER) | grep -v cset > $@

clean:
	rm -rf build

PERF=perf stat -B -e context-switches,cpu-migrations,LLC-loads,LLC-load-misses,LLC-stores,LLC-store-misses,L1-dcache-load-misses,L1-dcache-loads,L1-dcache-stores,L1-icache-load-misses,cache-misses,cache-references,dTLB-load-misses,dTLB-loads,dTLB-store-misses,dTLB-stores,iTLB-load-misses,iTLB-loads
PERF_PARAMS=--max_copy=128 --samples 500000000 --pipes=3 --stages=2

perf_flow:
	cargo build --release --bin null_rand
	cset shield --userset=sdr --exec -- $(PERF) cargo run --release --bin null_rand -- $(PERF_PARAMS) --scheduler flow

perf_smol:
	cargo build --release --bin null_rand
	cset shield --userset=sdr --exec -- $(PERF) cargo run --release --bin null_rand -- $(PERF_PARAMS) --scheduler smoln

perf_gr:
	cset shield --userset=sdr --exec -- $(PERF) ./build/null_rand_flowgraph $(PERF_PARAMS)
