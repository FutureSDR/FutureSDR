#!/bin/bash

set -xe

SYSTEM_CPUS=0,1,6,7

echo "==> Setting AllowedCPUs to: ${SYSTEM_CPUS}"

sudo systemctl set-property --runtime -- user.slice AllowedCPUs=${SYSTEM_CPUS}
sudo systemctl set-property --runtime -- system.slice AllowedCPUs=${SYSTEM_CPUS}
sudo systemctl set-property --runtime -- init.scope AllowedCPUs=${SYSTEM_CPUS}
