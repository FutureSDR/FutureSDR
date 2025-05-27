#!/bin/bash

# Get all available CPUs in 0-(n-1) format
ALL_CPUS="0-$(($(nproc) - 1))"

echo "==> Resetting AllowedCPUs to all CPUs: $ALL_CPUS"

sudo systemctl set-property --runtime system.slice AllowedCPUs=$ALL_CPUS
sudo systemctl set-property --runtime user.slice   AllowedCPUs=$ALL_CPUS
sudo systemctl set-property --runtime init.scope   AllowedCPUs=$ALL_CPUS
