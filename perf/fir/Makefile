SHELL=/bin/bash

GRRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/gr_{0}_4_{1}_{2}_legacy_.csv".format(*x) for x in itertools.product(range(10), np.arange(1,25,2), [20000000])]))')
FSRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/fs_{0}_4_{1}_{2}_{3}_.csv".format(*x) for x in itertools.product(range(5), np.arange(1,25,2), [20000000], ["flow", "smoln", "flow-spsc", "smoln-spsc", "local"])]))')

SHIELD=sudo systemd-run --uid=1000 --slice=sdr --wait -P -d

.PHONY: setup all clean perf_smol perf_flow perf_gr

all: setup $(GRRESULTS) $(FSRESULTS)

gr: setup $(GRRESULTS)
fs: setup $(FSRESULTS)

setup:
	@echo "### SETTING UP"
	../migrate-processes.sh

perf-data/gr%.csv: build/fir_rand_flowgraph
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval SAMPLES=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	$(eval SCHEDULER=$(shell python3 -c "print(\"$@\".split(\"_\")[5])"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo SAMPLES=$(SAMPLES)
	@echo SCHEDULER=$(SCHEDULER)

	$(SHIELD) -- ./build/fir_rand_flowgraph --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) > $@

perf-data/fs%.csv: fir.rs
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
