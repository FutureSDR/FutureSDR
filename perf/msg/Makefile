SHELL=/bin/bash

GRRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/gr_{0}_{1}_{1}_{2}_.csv".format(*x) for x in itertools.product(range(10), np.arange(1,21), [1000])]))')
FSRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/fs_{0}_{1}_{1}_{2}_.csv".format(*x) for x in itertools.product(range(10), np.arange(1,21), [1000])]))')

REPETITIONS=10

SHIELD=sudo systemd-run --uid=1000 --slice=sdr --wait -P -p AllowedCPUs=6-11 -d

.PHONY: setup all clean

all: setup $(GRRESULTS) $(FSRESULTS)

gr: setup $(GRRESULTS) 
fs: setup $(FSRESULTS) 

setup:
	@echo "### SETTING UP"
	../create_cpuset.sh
	../cpugov.sh performance

build/msg_flowgraph: msg_flowgraph.cpp msg_flowgraph.hpp
	rm -rf build
	mkdir build
	cd build && cmake .. && make

perf-data/gr%.csv: build/msg_flowgraph
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval BURST_SIZE=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo BURST_SIZE=$(BURST_SIZE)

	$(SHIELD) -- ./build/msg_flowgraph --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --repetitions=$(REPETITIONS) --burst_size=$(BURST_SIZE) > $@

perf-data/fs%.csv: msg.rs
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python3 -c "print(\"$@\".split(\"_\")[1])"))
	$(eval PIPES=$(shell python3 -c "print(\"$@\".split(\"_\")[2])"))
	$(eval STAGES=$(shell python3 -c "print(\"$@\".split(\"_\")[3])"))
	$(eval BURST_SIZE=$(shell python3 -c "print(\"$@\".split(\"_\")[4])"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo BURST_SIZE=$(BURST_SIZE)

	$(SHIELD) -- cargo run --release -- --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --repetitions=$(REPETITIONS) --burst-size=$(BURST_SIZE) > $@

clean:
	rm -rf build
