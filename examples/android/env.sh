export TOOLCHAIN_ROOT=${HOME}/Android/Sdk/ndk/21.3.6528147
export HOST_ARCH=linux-x86_64
export BUILD_ROOT=/home/basti/src/gnuradio-android

#############################################################
### DERIVED CONFIG
#############################################################
export SYS_ROOT=${TOOLCHAIN_ROOT}/sysroot
# the variable has to be set to allow cross-compilation
# but the paths in .pc files are absolute
export PKG_CONFIG_SYSROOT_DIR=/
export TOOLCHAIN_BIN=${TOOLCHAIN_ROOT}/toolchains/llvm/prebuilt/${HOST_ARCH}/bin
export API_LEVEL=29
export CC="${TOOLCHAIN_BIN}/aarch64-linux-android${API_LEVEL}-clang"
export LD=${TOOLCHAIN_BIN}/aarch64-linux-android-ld
export AR=${TOOLCHAIN_BIN}/aarch64-linux-android-ar
export RANLIB=${TOOLCHAIN_BIN}/aarch64-linux-android-ranlib
export STRIP=${TOOLCHAIN_BIN}/aarch64-linux-android-strip
export PATH=${TOOLCHAIN_BIN}:${PATH}
export PREFIX=${BUILD_ROOT}/toolchain/arm64-v8a
export PKG_CONFIG_PATH=${PREFIX}/lib/pkgconfig
export NCORES=$(getconf _NPROCESSORS_ONLN)

export TARGET_CC="${TOOLCHAIN_BIN}/aarch64-linux-android${API_LEVEL}-clang"
export TARGET_AR=${TOOLCHAIN_BIN}/aarch64-linux-android-ar
