SHELL=/bin/bash

GRRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/gr_{0}_8_{1}_{2}_{3}_legacy_.csv".format(*x) for x in itertools.product(range(20), np.arange(1,25,2), [20000000], [512])]))')
FSRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/fs_{0}_8_{1}_{2}_{3}_{4}_.csv".format(*x) for x in itertools.product(range(20), np.arange(1,25,2), [20000000], [512], ["smoln"])]))')

SHIELD=sudo systemd-run --uid=1000 --slice=sdr --wait -P -p AllowedCPUs=2,3,4,5,8,9,10,11 -d

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
	$(eval MAX=$(shell python3 -c "print(\"$@\".split(\"_\")[5])"))
	$(eval SCHEDULER=$(shell python3 -c "print(\"$@\".split(\"_\")[6])"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo SAMPLES=$(SAMPLES)
	@echo MAX=$(MAX)
	@echo SCHEDULER=$(SCHEDULER)

	$(SHIELD) -- ./build/fir_rand_flowgraph --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --max_copy=$(MAX) > $@

perf-data/fs%.csv: fir.rs
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

	$(SHIELD) -- cargo run --release -- --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --max-copy=$(MAX) --scheduler=$(SCHEDULER) > $@

clean:
	rm -rf build
