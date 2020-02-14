SHELL=/bin/bash

GRRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/gr_{0}_{1}_{1}_{2}_.csv".format(*x) for x in itertools.product(range(10), np.arange(1,21), [1000])]))')
NRRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/nr_{0}_{1}_{1}_{2}_.csv".format(*x) for x in itertools.product(range(10), np.arange(1,21), [1000])]))')

REPETITIONS=100

.PHONY: setup all clean

all: setup $(GRRESULTS) $(NRRESULTS)

gr: setup $(GRRESULTS) 
nr: setup $(NRRESULTS) 

setup:
	@echo "### SETTING UP"
	../create_cpuset.sh
	../cpugov.sh performance
	../irq_affinity.sh

build/msg_flowgraph: msg_flowgraph.cpp msg_flowgraph.hpp
	rm -rf build
	mkdir build
	cd build && cmake .. && make

perf-data/gr%.csv: build/msg_flowgraph
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python -c "print \"$@\".split(\"_\")[1]"))
	$(eval PIPES=$(shell python -c "print \"$@\".split(\"_\")[2]"))
	$(eval STAGES=$(shell python -c "print \"$@\".split(\"_\")[3]"))
	$(eval BURST_SIZE=$(shell python -c "print \"$@\".split(\"_\")[4]"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo BURST_SIZE=$(BURST_SIZE)

	cset shield --userset=sdr --exec -- ./build/msg_flowgraph --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --repetitions=$(REPETITIONS) --burst_size=$(BURST_SIZE) | grep -v cset > $@

perf-data/nr%.csv: msg.rs
	mkdir -p perf-data
	@echo $@
	$(eval RUN=$(shell python -c "print \"$@\".split(\"_\")[1]"))
	$(eval PIPES=$(shell python -c "print \"$@\".split(\"_\")[2]"))
	$(eval STAGES=$(shell python -c "print \"$@\".split(\"_\")[3]"))
	$(eval BURST_SIZE=$(shell python -c "print \"$@\".split(\"_\")[4]"))
	@echo RUN=$(RUN)
	@echo PIPES=$(PIPES)
	@echo STAGES=$(STAGES)
	@echo BURST_SIZE=$(BURST_SIZE)

	cset shield --userset=sdr --exec -- cargo run --release -- --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --repetitions=$(REPETITIONS) --burst_size=$(BURST_SIZE) | grep -v cset > $@

clean:
	rm -rf build
