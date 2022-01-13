#!/bin/bash

set -xe

unset LD_LIBRARY_PATH
. ~/Downloads/sdk/environment-setup-aarch64-xilinx-linux

cargo "$@"

