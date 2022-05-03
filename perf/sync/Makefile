SHELL=/bin/bash

FSRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/fs_{0}_6_{1}_{2}_{3}_{4}_{5}_{6}_.csv".format(*x) for x in itertools.product(range(20), np.arange(1,25,2), [200000000], [512], ["smol1", "smoln", "flow"], ["circ"], ["async", "sync"])]))')

.PHONY: setup all

all: setup $(FSRESULTS)

setup:
	@echo "### SETTING UP"
	../create_cpuset.sh
	../cpugov.sh performance
	../irq_affinity.sh

perf-data/fs%.csv: sync.rs
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval SAMPLES=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	$(eval MAX=$(shell python3 -c "print(\"$@\".split(\"_\")[5])"))
	$(eval SCHEDULER=$(shell python3 -c "print(\"$@\".split(\"_\")[6])"))
	$(eval BUFFER_FLAG=$(shell python3 -c "print(\"--slab\" if \"$@\".split(\"_\")[7] == \"slab\" else \"\")"))
	$(eval SYNC_FLAG=$(shell python3 -c "print(\"--sync\" if \"$@\".split(\"_\")[8] == \"sync\" else \"\")"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo SAMPLES=$(SAMPLES)
	@echo MAX=$(MAX)
	@echo SCHEDULER=$(SCHEDULER)
	@echo BUFFER_FLAG=$(BUFFER_FLAG)
	@echo SYNC_FLAG=$(SYNC_FLAG)

	cset shield --userset=sdr --exec -- cargo run --release -- --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --max_copy=$(MAX) --scheduler=$(SCHEDULER) $(BUFFER_FLAG) $(SYNC_FLAG) | grep -v cset > $@

