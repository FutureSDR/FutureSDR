SHELL=/bin/bash

FSRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/fs_{0}_6_{1}_{2}_{3}_{4}_{5}_.csv".format(*x) for x in itertools.product(range(20), [6], [3000000000], [4192, 8384, 12576, 16768, 20960, 25152, 29344, 33536, 37728, 41920, 46112, 50304, 54496, 58688, 62880, 67072, 71264, 75456, 79648], ["flow"], ["circ", "slab"])]))')

.PHONY: setup all

all: setup $(FSRESULTS)

setup:
	@echo "### SETTING UP"
	../create_cpuset.sh
	../cpugov.sh performance
	../irq_affinity.sh

perf-data/fs%.csv: buffer_size.rs
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval SAMPLES=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	$(eval BUFFER_SIZE=$(shell python3 -c "print(\"$@\".split(\"_\")[5])"))
	$(eval SCHEDULER=$(shell python3 -c "print(\"$@\".split(\"_\")[6])"))
	$(eval BUFFER=$(shell python3 -c "print(\"$@\".split(\"_\")[7])"))
	$(eval BUFFER_FLAG=$(shell python3 -c "print(\"--slab\" if \"$@\".split(\"_\")[7] == \"slab\" else \"\")"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo SAMPLES=$(SAMPLES)
	@echo SCHEDULER=$(SCHEDULER)
	@echo BUFFER_SIZE=$(BUFFER_SIZE)
	@echo BUFFER=$(BUFFER)
	@echo BUFFER_FLAG=$(BUFFER_FLAG)

	cset shield --userset=sdr --exec -- cargo run --release -- --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --scheduler=$(SCHEDULER) --buffer-size=$(BUFFER_SIZE) $(BUFFER_FLAG) | grep -v cset > $@

