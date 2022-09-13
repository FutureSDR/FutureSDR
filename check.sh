#!/bin/bash

set -xe

SCRIPT=$(readlink -f $0)
SCRIPTPATH=`dirname $SCRIPT`

###########################################################
# FMT
###########################################################
cd ${SCRIPTPATH} && cargo fmt --check
cd ${SCRIPTPATH}/pmt && cargo fmt --check
cd ${SCRIPTPATH}/frontend && cargo fmt --check

# perf
cd ${SCRIPTPATH}/perf/buffer_rand && cargo fmt --check
cd ${SCRIPTPATH}/perf/buffer_size && cargo fmt --check
cd ${SCRIPTPATH}/perf/fir && cargo fmt --check
cd ${SCRIPTPATH}/perf/fir_latency && cargo fmt --check
cd ${SCRIPTPATH}/perf/msg && cargo fmt --check
cd ${SCRIPTPATH}/perf/null_rand && cargo fmt --check
cd ${SCRIPTPATH}/perf/null_rand_latency && cargo fmt --check
cd ${SCRIPTPATH}/perf/vulkan && cargo fmt --check
cd ${SCRIPTPATH}/perf/wgpu && cargo fmt --check
cd ${SCRIPTPATH}/perf/zynq && cargo fmt --check

# examples
cd ${SCRIPTPATH}/examples/android && cargo fmt --check
cd ${SCRIPTPATH}/examples/android-hw && cargo fmt --check
cd ${SCRIPTPATH}/examples/audio && cargo fmt --check
cd ${SCRIPTPATH}/examples/custom-routes && cargo fmt --check
cd ${SCRIPTPATH}/examples/cw && cargo fmt --check
cd ${SCRIPTPATH}/examples/firdes && cargo fmt --check
cd ${SCRIPTPATH}/examples/fm-receiver && cargo fmt --check
cd ${SCRIPTPATH}/examples/logging && cargo fmt --check
cd ${SCRIPTPATH}/examples/rx-to-file && cargo fmt --check
cd ${SCRIPTPATH}/examples/spectrum && cargo fmt --check
cd ${SCRIPTPATH}/examples/wasm && cargo fmt --check
cd ${SCRIPTPATH}/examples/wlan && cargo fmt --check
cd ${SCRIPTPATH}/examples/wgpu && cargo fmt --check
cd ${SCRIPTPATH}/examples/zeromq && cargo fmt --check
cd ${SCRIPTPATH}/examples/zigbee && cargo fmt --check

###########################################################
# CLIPPY
###########################################################
cd ${SCRIPTPATH} && cargo clippy --all-targets --workspace --features=vulkan,zeromq,audio,flow_scheduler,tpb_scheduler,soapy,lttng,zynq,wgpu -- -D warnings
cd ${SCRIPTPATH} && RUSTFLAGS='--cfg=web_sys_unstable_apis' cargo clippy --lib --workspace --features=audio,wgpu --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/pmt && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/frontend && cargo clippy --all-targets --target=wasm32-unknown-unknown -- -D warnings

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
cd ${SCRIPTPATH}/perf/wgpu && RUSTFLAGS='--cfg=web_sys_unstable_apis' cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/perf/zynq && cargo clippy --all-targets -- -D warnings

# examples
cd ${SCRIPTPATH}/examples/android && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/android-hw && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/audio && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/custom-routes && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/cw && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/cw && cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/examples/firdes && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/fm-receiver && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/logging && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/rx-to-file && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/spectrum && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/spectrum && RUSTFLAGS='--cfg=web_sys_unstable_apis' cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/examples/wasm && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/wasm && RUSTFLAGS='--cfg=web_sys_unstable_apis' cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/examples/wlan && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/wgpu && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/wgpu && RUSTFLAGS='--cfg=web_sys_unstable_apis' cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/examples/zeromq && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/zigbee && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/zigbee && RUSTFLAGS='--cfg=web_sys_unstable_apis' cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings

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
cd ${SCRIPTPATH}/examples/custom-routes && cargo test --all-targets
cd ${SCRIPTPATH}/examples/cw && cargo test --all-targets
cd ${SCRIPTPATH}/examples/firdes && cargo test --all-targets
cd ${SCRIPTPATH}/examples/fm-receiver && cargo test --all-targets
cd ${SCRIPTPATH}/examples/logging && cargo test --all-targets
cd ${SCRIPTPATH}/examples/rx-to-file && cargo test --all-targets
cd ${SCRIPTPATH}/examples/spectrum && cargo test --all-targets
cd ${SCRIPTPATH}/examples/wasm && cargo test --all-targets
cd ${SCRIPTPATH}/examples/wlan && cargo test --all-targets
cd ${SCRIPTPATH}/examples/wgpu && cargo test --all-targets
cd ${SCRIPTPATH}/examples/zeromq && cargo test --all-targets
cd ${SCRIPTPATH}/examples/zigbee && cargo test --all-targets
