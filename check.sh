#!/bin/bash

SCRIPT=$(readlink -f $0)
SCRIPTPATH=`dirname $SCRIPT`

cd ${SCRIPTPATH} && cargo fmt
cd ${SCRIPTPATH}/perf/buffer_rand && cargo fmt
cd ${SCRIPTPATH}/perf/buffer_size && cargo fmt
cd ${SCRIPTPATH}/perf/fir && cargo fmt
cd ${SCRIPTPATH}/perf/fir_latency && cargo fmt
cd ${SCRIPTPATH}/perf/msg && cargo fmt
cd ${SCRIPTPATH}/perf/null_rand && cargo fmt
cd ${SCRIPTPATH}/perf/null_rand_latency && cargo fmt
cd ${SCRIPTPATH}/perf/vulkan && cargo fmt
cd ${SCRIPTPATH}/perf/wgpu && cargo fmt
cd ${SCRIPTPATH}/perf/zynq && cargo fmt
