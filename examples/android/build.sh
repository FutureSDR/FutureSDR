#!/bin/bash

set -xe

#############################################################
### CONFIG
#############################################################
export TOOLCHAIN_ROOT=${HOME}/Android/Sdk/ndk/28.2.13676358
export HOST_ARCH=linux-x86_64
export PREFIX=${HOME}/src/android-sdr-toolchain/toolchain/arm64-v8a
export API_LEVEL=29

#############################################################
### DERIVED CONFIG
#############################################################
export SOAPYSDR_NO_PKG_CONFIG=1
export SOAPY_SDR_ROOT=${PREFIX}
export CMAKE_POLICY_VERSION_MINIMUM=3.5

export TOOLCHAIN_BIN=${TOOLCHAIN_ROOT}/toolchains/llvm/prebuilt/${HOST_ARCH}/bin
export CC="${TOOLCHAIN_BIN}/aarch64-linux-android${API_LEVEL}-clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="${TOOLCHAIN_BIN}/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${TOOLCHAIN_BIN}/aarch64-linux-android${API_LEVEL}-clang"

#############################################################
### BUILD
#############################################################
cargo build --target aarch64-linux-android --lib --release

TARGET_DIR=$(cargo metadata --format-version=1 --no-deps | jq -r '.target_directory')
cp ${TARGET_DIR}/aarch64-linux-android/release/libfuturesdr_android.so ./FutureSDR/app/src/main/jni/arm64-v8a/

