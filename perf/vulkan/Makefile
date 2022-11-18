SHELL=/bin/bash

FSRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/fs_{0}_{1}_{2}_.csv".format(*x) for x in itertools.product(range(20), [100000000], [4192, 8384, 12576, 16768, 20960, 25152, 29344, 33536, 37728, 41920, 46112, 50304, 54496, 58688, 62880, 67072, 71264, 75456, 79648])]))')

.PHONY: setup all

all: setup $(FSRESULTS)

setup:
	@echo "### SETTING UP"
	../create_cpuset.sh
	../cpugov.sh performance
	../irq_affinity.sh

perf-data/fs%.csv: vulkan.rs
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval SAMPLES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval BUFFER_SIZE=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	@echo RUN=$(RUN)
	@echo SAMPLES=$(SAMPLES)
	@echo BUFFER_SIZE=$(BUFFER_SIZE)

	cset shield --userset=sdr --exec -- cargo run --release -- --run=$(RUN) --samples=$(SAMPLES) --buffer-size=$(BUFFER_SIZE) | grep -v cset > $@

