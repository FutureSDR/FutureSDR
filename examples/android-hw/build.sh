#!/bin/bash

source ./env.sh
RUSTFLAGS="-C target-cpu=generic" cargo build --target aarch64-linux-android --lib
