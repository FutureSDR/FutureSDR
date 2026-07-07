SHELL=/bin/bash

SHIELD=sudo systemd-run --uid=1000 --slice=sdr --wait -P -d

FSRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/fs_{0}_4_{1}_{2}_{3}_{4}_{5}_{6}_.csv".format(*x) for x in itertools.product(range(20), [12], [1000000000], [128], [4192, 8384, 12576, 16768, 20960, 25152, 29344, 33536, 37728, 41920, 46112, 50304, 54496, 58688, 62880, 67072, 71264, 75456, 79648], ["flow"], ["circ", "slab", "spsc"])]))')

.PHONY: setup all

all: setup $(FSRESULTS)

setup:
	@echo "### SETTING UP"
	../perf_setup.sh

perf-data/fs%.csv: buffer_size.rs Cargo.toml
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval SAMPLES=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	$(eval CHUNK=$(shell python3 -c "print(\"$@\".split(\"_\")[5])"))
	$(eval BUFFER_SIZE=$(shell python3 -c "print(\"$@\".split(\"_\")[6])"))
	$(eval CONFIG=$(shell python3 -c "print(\"$@\".split(\"_\")[7])"))
	$(eval BUFFER=$(shell python3 -c "print(\"$@\".split(\"_\")[8])"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo SAMPLES=$(SAMPLES)
	@echo CHUNK=$(CHUNK)
	@echo CONFIG=$(CONFIG)
	@echo BUFFER_SIZE=$(BUFFER_SIZE)
	@echo BUFFER=$(BUFFER)

	$(SHIELD) -- cargo run --release -- --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --chunk=$(CHUNK) --config=$(CONFIG) --buffer-size=$(BUFFER_SIZE) --buffer=$(BUFFER) > $@
