#!/bin/bash

set -xe

SCRIPT=$(readlink -f $0)
SCRIPTPATH=`dirname $SCRIPT`

cd ${SCRIPTPATH} && find . -name "Cargo.lock" -delete

CARGO_FMT="cargo +nightly fmt"

###########################################################
# FMT
###########################################################
cd ${SCRIPTPATH} && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/crates/futuredsp && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/crates/macros && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/crates/prophecy && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/crates/remote && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/crates/types && ${CARGO_FMT} --check

# perf
cd ${SCRIPTPATH}/perf/buffer_rand && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/buffer_size && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/burn && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/fir && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/fir_latency && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/msg && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/null_rand && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/null_rand_latency && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/perf && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/vulkan && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/wgpu && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/perf/zynq && ${CARGO_FMT} --check

# examples
cd ${SCRIPTPATH}/examples/adsb && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/android && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/audio && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/burn && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/custom-routes && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/cw && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/egui && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/file-trx && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/firdes && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/fm-receiver && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/inplace && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/keyfob && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/logging && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/lora && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/m17 && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/macros && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/rattlegram && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/spectrum && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/ssb && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/wasm && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/wgpu && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/wlan && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/zeromq && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/zigbee && ${CARGO_FMT} --check
cd ${SCRIPTPATH}/examples/zynq && ${CARGO_FMT} --check

###########################################################
# CLIPPY
###########################################################
# aaronia feature is not tested, since most user might not have the sdr installed
cd ${SCRIPTPATH} && cargo clippy --all-targets --workspace --features=burn,vulkan,zeromq,audio,flow_scheduler,soapy,zynq,wgpu,seify_dummy -- -D warnings
cd ${SCRIPTPATH}/crates/futuredsp && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/crates/macros && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/crates/remote && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/crates/types && cargo clippy --all-targets -- -D warnings

# perf
cd ${SCRIPTPATH}/perf/buffer_rand && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/buffer_size && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/burn && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/fir && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/msg && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/null_rand && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/perf && cargo clippy --all-targets --all-features -- -D warnings
cd ${SCRIPTPATH}/perf/vulkan && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/perf/wgpu && cargo clippy --all-targets -- -D warnings
if [[ "$OSTYPE" == linux* ]]; then
  cd ${SCRIPTPATH}/perf/fir_latency && cargo clippy --all-targets -- -D warnings
  cd ${SCRIPTPATH}/perf/null_rand_latency && cargo clippy --all-targets -- -D warnings
  cd ${SCRIPTPATH}/perf/zynq && cargo clippy --all-targets -- -D warnings
fi

# examples
cd ${SCRIPTPATH}/examples/adsb && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/android && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/audio && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/burn && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/custom-routes && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/cw && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/egui && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/file-trx && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/firdes && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/fm-receiver && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/inplace && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/keyfob && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/logging && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/lora && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/m17 && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/macros && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/rattlegram && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/spectrum && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/ssb && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/wasm && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/wgpu && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/wlan && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/zeromq && cargo clippy --all-targets -- -D warnings
cd ${SCRIPTPATH}/examples/zigbee && cargo clippy --all-targets -- -D warnings
if [[ "$OSTYPE" == linux* ]]; then
  cd ${SCRIPTPATH}/examples/zynq && cargo clippy --all-targets -- -D warnings
fi

# WASM
cd ${SCRIPTPATH} && cargo clippy --lib --workspace --features=burn,audio,seify_dummy,wgpu --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/crates/macros && cargo clippy --all-targets --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/crates/prophecy && cargo clippy --all-targets --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/crates/types && cargo clippy --all-targets --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/perf/wgpu && cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/examples/cw && cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/examples/rattlegram && cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/examples/spectrum && cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/examples/wasm && cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/examples/wgpu && cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings
cd ${SCRIPTPATH}/examples/zigbee && cargo clippy --lib --target=wasm32-unknown-unknown -- -D warnings

###########################################################
# Test
###########################################################
# aaronia feature is not tested, since most user might not have the sdr installed
cd ${SCRIPTPATH} && cargo test --all-targets --workspace --features=vulkan,zeromq,audio,flow_scheduler,seify_dummy,soapy,wgpu,zynq -j 4
cd ${SCRIPTPATH}/crates/futuredsp && cargo test --all-targets
cd ${SCRIPTPATH}/crates/macros && cargo test --all-targets
cd ${SCRIPTPATH}/crates/remote && cargo test --all-targets
cd ${SCRIPTPATH}/crates/types && cargo test --all-targets

# perf
cd ${SCRIPTPATH}/perf/buffer_rand && cargo test --all-targets
cd ${SCRIPTPATH}/perf/buffer_size && cargo test --all-targets
cd ${SCRIPTPATH}/perf/burn && cargo test --all-targets
cd ${SCRIPTPATH}/perf/fir && cargo test --all-targets
cd ${SCRIPTPATH}/perf/msg && cargo test --all-targets
cd ${SCRIPTPATH}/perf/null_rand && cargo test --all-targets
cd ${SCRIPTPATH}/perf/perf && cargo test --all-targets --all-features
cd ${SCRIPTPATH}/perf/vulkan && cargo test --all-targets
cd ${SCRIPTPATH}/perf/wgpu && cargo test --all-targets
if [[ "$OSTYPE" == linux* ]]; then
  cd ${SCRIPTPATH}/perf/fir_latency && cargo test --all-targets
  cd ${SCRIPTPATH}/perf/null_rand_latency && cargo test --all-targets
  cd ${SCRIPTPATH}/perf/zynq && cargo test --all-targets
fi

# examples
cd ${SCRIPTPATH}/examples/adsb && cargo test --all-targets
cd ${SCRIPTPATH}/examples/android && cargo test --all-targets
cd ${SCRIPTPATH}/examples/audio && cargo test --all-targets
cd ${SCRIPTPATH}/examples/burn && cargo test --all-targets
cd ${SCRIPTPATH}/examples/custom-routes && cargo test --all-targets
cd ${SCRIPTPATH}/examples/cw && cargo test --all-targets
cd ${SCRIPTPATH}/examples/egui && cargo test --all-targets
cd ${SCRIPTPATH}/examples/firdes && cargo test --all-targets
cd ${SCRIPTPATH}/examples/fm-receiver && cargo test --all-targets
cd ${SCRIPTPATH}/examples/inplace && cargo test --all-targets
cd ${SCRIPTPATH}/examples/keyfob && cargo test --all-targets
cd ${SCRIPTPATH}/examples/logging && cargo test --all-targets
cd ${SCRIPTPATH}/examples/lora && cargo test --all-targets
cd ${SCRIPTPATH}/examples/m17 && cargo test --all-targets
cd ${SCRIPTPATH}/examples/macros && cargo test --all-targets
cd ${SCRIPTPATH}/examples/rattlegram && cargo test --all-targets
cd ${SCRIPTPATH}/examples/file-trx && cargo test --all-targets
cd ${SCRIPTPATH}/examples/spectrum && cargo test --all-targets
cd ${SCRIPTPATH}/examples/ssb && cargo test --all-targets
cd ${SCRIPTPATH}/examples/wasm && cargo test --all-targets
cd ${SCRIPTPATH}/examples/wgpu && cargo test --all-targets
cd ${SCRIPTPATH}/examples/wlan && cargo test --all-targets
cd ${SCRIPTPATH}/examples/zeromq && cargo test --all-targets
cd ${SCRIPTPATH}/examples/zigbee && cargo test --all-targets
if [[ "$OSTYPE" == linux* ]]; then
  cd ${SCRIPTPATH}/examples/zynq && cargo test --all-targets
fi
