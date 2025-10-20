#!/bin/bash

set -xe

source ./env.sh

rm -f ./FutureSDR/app/src/main/jni/arm64-v8a/libfuturesdr_android.so
ln -s ${HOME}/.cargo/target/aarch64-linux-android/release/libfuturesdr_android.so ./FutureSDR/app/src/main/jni/arm64-v8a/

cargo build --target aarch64-linux-android --lib --release
