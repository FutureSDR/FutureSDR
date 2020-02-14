#!/bin/bash

cargo install cross
RUSTFLAGS="-C target-cpu=generic" cross build --example zynq --target=aarch64-unknown-linux-gnu --no-default-features --features=zynq
