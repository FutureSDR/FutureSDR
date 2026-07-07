#!/bin/bash

set -xe

ALL_CPUS="0-$(($(nproc --all) - 1))"

echo "==> Resetting AllowedCPUs to all CPUs: $ALL_CPUS"

sudo systemctl set-property --runtime system.slice AllowedCPUs=$ALL_CPUS
sudo systemctl set-property --runtime user.slice   AllowedCPUs=$ALL_CPUS
sudo systemctl set-property --runtime init.scope   AllowedCPUs=$ALL_CPUS

sudo systemctl stop sdr.slice || true
sudo systemctl reset-failed sdr.slice || true

echo "==> Setting CPU governor to powersave"
for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
  governor="$cpu/cpufreq/scaling_governor"
  if [[ -e "$governor" ]]; then
    echo powersave | sudo tee "$governor" > /dev/null
  fi
done
