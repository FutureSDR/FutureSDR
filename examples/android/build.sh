#!/bin/bash

set -xe

source ./env.sh

ln -s ${HOME}/.cargo/target/aarch64-linux-android/debug/libfuturesdr_android.so ${PREFIX}/lib/ || true

cargo build --target aarch64-linux-android --lib
