SHELL=/bin/bash

GRRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/gr_{0}_6_{1}_{2}_{3}_legacy_.csv".format(*x) for x in itertools.product(range(20), np.arange(1,25,2), [200000000], [512])]))')
FSRESULTS=$(shell python3 -c 'import itertools; import numpy as np; print(" ".join(["perf-data/fs_{0}_6_{1}_{2}_{3}_{4}_.csv".format(*x) for x in itertools.product(range(20), np.arange(1,25,2), [200000000], [512], ["smoln", "flow"])]))')

.PHONY: setup all clean run_trace run_no_trace

all: setup $(GRRESULTS) $(FSRESULTS)

gr: setup $(GRRESULTS)
fs: setup $(FSRESULTS)

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

	lttng stop || true
	lttng destroy || true
	rm -rf perf-data/foo

	lttng create foo -o perf-data/foo
	lttng enable-event --userspace null_rand_latency:tx
	lttng enable-event --userspace null_rand_latency:rx
	lttng start

	cset shield --userset=sdr --exec -- ./build/null_rand_flowgraph --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --max_copy=$(MAX)

	lttng destroy
	python3 parse.py perf-data/foo > $@
	rm -rf perf-data/foo

perf-data/fs%.csv: null_rand_latency.rs
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

	lttng stop || true
	lttng destroy || true
	rm -rf perf-data/foo

	lttng create foo -o perf-data/foo
	lttng enable-event --userspace futuresdr:tx
	lttng enable-event --userspace futuresdr:rx
	lttng start

	cset shield --userset=sdr --exec -- cargo run --release -- --run=$(RUN) --pipes=$(PIPES) --stages=$(STAGES) --samples=$(SAMPLES) --max-copy=$(MAX) --scheduler=$(SCHEDULER)

	lttng destroy
	python3 parse.py perf-data/foo > $@
	rm -rf perf-data/foo

clean:
	rm -rf build

# run_no_trace:
# 	lttng stop || true
# 	cset shield --userset=sdr --exec -- cargo run --release -- --pipes=6 --stages=6 --samples=1000000000 --max_copy=512 --scheduler=flow

# run_trace:
# 	mkdir -p perf-data
# 	lttng create foo -o perf-data/foo
# 	lttng enable-event --userspace null_rand_latency:tx
# 	lttng enable-event --userspace null_rand_latency:rx
# 	lttng start

# 	cset shield --userset=sdr --exec -- cargo run --release -- --pipes=6 --stages=6 --samples=1000000000 --max_copy=512 --scheduler=flow

# 	lttng stop
