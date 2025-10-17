#!/bin/bash

set -xe

source ./env.sh

rm -f ${PREFIX}/lib/libfuturesdr_android.so
ln -s ${HOME}/.cargo/target/aarch64-linux-android/release/libfuturesdr_android.so ${PREFIX}/lib/

cargo build --target aarch64-linux-android --lib --release
