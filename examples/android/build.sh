#!/bin/bash

set -xe

source ./env.sh

ln -s ${PREFIX}/lib/SoapySDR/modules0.8/librtlsdrSupport.so ${PREFIX}/lib/ || true
ln -s ${HOME}/.cargo/target/aarch64-linux-android/debug/libandroidhw.so ${PREFIX}/lib/ || true

RUSTFLAGS="-L $(pwd) -C target-cpu=generic" cargo build --target aarch64-linux-android --lib
