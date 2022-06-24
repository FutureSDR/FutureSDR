#!/bin/bash

set -xe

SCRIPT=$(readlink -f $0)
SCRIPTPATH=`dirname $SCRIPT`

###########################################################
# FMT
###########################################################
cd ${SCRIPTPATH} && cargo fmt

# perf
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

# examples
cd ${SCRIPTPATH}/examples/android && cargo fmt
cd ${SCRIPTPATH}/examples/android-hw && cargo fmt
cd ${SCRIPTPATH}/examples/audio && cargo fmt
cd ${SCRIPTPATH}/examples/cw && cargo fmt
cd ${SCRIPTPATH}/examples/firdes && cargo fmt
cd ${SCRIPTPATH}/examples/fm-receiver && cargo fmt
cd ${SCRIPTPATH}/examples/logging && cargo fmt
cd ${SCRIPTPATH}/examples/rx-to-file && cargo fmt
cd ${SCRIPTPATH}/examples/spectrum && cargo fmt
cd ${SCRIPTPATH}/examples/wasm && cargo fmt
cd ${SCRIPTPATH}/examples/wgpu && cargo fmt
cd ${SCRIPTPATH}/examples/zeromq && cargo fmt
cd ${SCRIPTPATH}/examples/zigbee && cargo fmt

###########################################################
# CLIPPY
###########################################################
cd ${SCRIPTPATH} && cargo clippy --all-targets --workspace --features=vulkan,zeromq,audio,flow_scheduler,tpb_scheduler,soapy,lttng,zynq,wgpu -- -D warnings

# perf
cd ${SCRIPTPATH}/perf/buffer_rand && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/buffer_size && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/fir && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/fir_latency && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/msg && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/null_rand && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/null_rand_latency && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/vulkan && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/wgpu && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/zynq && cargo clippy --all-targets -- -D warnings

# examples
cd ${SCRIPTPATH}/examples/android && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/android-hw && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/audio && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/cw && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/firdes && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/fm-receiver && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/logging && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/rx-to-file && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/spectrum && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/wasm && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/wgpu && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/zeromq && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/zigbee && cargo clippy --all-targets -- -D warnings

###########################################################
# Test
###########################################################
cd ${SCRIPTPATH} && cargo test --all-targets --workspace --features=vulkan,zeromq,audio,flow_scheduler,tpb_scheduler,soapy,lttng,zynq,wgpu -j 4

# perf
cd ${SCRIPTPATH}/perf/buffer_rand && cargo test --all-targets
cd ${SCRIPTPATH}/perf/buffer_size && cargo test --all-targets
cd ${SCRIPTPATH}/perf/fir && cargo test --all-targets
cd ${SCRIPTPATH}/perf/fir_latency && cargo test --all-targets
cd ${SCRIPTPATH}/perf/msg && cargo test --all-targets
cd ${SCRIPTPATH}/perf/null_rand && cargo test --all-targets
cd ${SCRIPTPATH}/perf/null_rand_latency && cargo test --all-targets
cd ${SCRIPTPATH}/perf/vulkan && cargo test --all-targets
cd ${SCRIPTPATH}/perf/wgpu && cargo test --all-targets
cd ${SCRIPTPATH}/perf/zynq && cargo test --all-targets

# examples
cd ${SCRIPTPATH}/examples/android && cargo test --all-targets
cd ${SCRIPTPATH}/examples/android-hw && cargo test --all-targets
cd ${SCRIPTPATH}/examples/audio && cargo test --all-targets
cd ${SCRIPTPATH}/examples/cw && cargo test --all-targets
cd ${SCRIPTPATH}/examples/firdes && cargo test --all-targets
cd ${SCRIPTPATH}/examples/fm-receiver && cargo test --all-targets
cd ${SCRIPTPATH}/examples/logging && cargo test --all-targets
cd ${SCRIPTPATH}/examples/rx-to-file && cargo test --all-targets
cd ${SCRIPTPATH}/examples/spectrum && cargo test --all-targets
cd ${SCRIPTPATH}/examples/wasm && cargo test --all-targets
cd ${SCRIPTPATH}/examples/wgpu && cargo test --all-targets
cd ${SCRIPTPATH}/examples/zeromq && cargo test --all-targets
cd ${SCRIPTPATH}/examples/zigbee && cargo test --all-targets
