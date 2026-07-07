#!/bin/bash

set -xe

SYSTEM_CPUS=0,1,6,7
SDR_CPUS=2,3,4,5

echo "==> Setting CPU governor to performance"
for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
  governor="$cpu/cpufreq/scaling_governor"
  if [[ -e "$governor" ]]; then
    echo performance | sudo tee "$governor" > /dev/null
  fi
done

echo "==> Setting AllowedCPUs to: ${SYSTEM_CPUS}"
sudo systemctl set-property --runtime sdr.slice AllowedCPUs=${SDR_CPUS}
sudo systemctl set-property --runtime user.slice AllowedCPUs=${SYSTEM_CPUS}
sudo systemctl set-property --runtime system.slice AllowedCPUs=${SYSTEM_CPUS}
sudo systemctl set-property --runtime init.scope AllowedCPUs=${SYSTEM_CPUS}
