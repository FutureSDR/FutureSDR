#!/bin/bash

set -xe

ALL_CPUS="0-$(($(nproc --all) - 1))"

echo "==> Resetting AllowedCPUs to all CPUs: $ALL_CPUS"

sudo systemctl set-property --runtime system.slice AllowedCPUs=$ALL_CPUS
sudo systemctl set-property --runtime user.slice   AllowedCPUs=$ALL_CPUS
sudo systemctl set-property --runtime init.scope   AllowedCPUs=$ALL_CPUS
