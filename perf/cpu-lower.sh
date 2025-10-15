#!/bin/bash

set -e

echo "==> Configuring CPU for reproducible performance measurements (intel_pstate driver)"

# Check driver
DRIVER=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_driver)
if [[ "$DRIVER" != "intel_pstate" ]]; then
  echo "Error: Expected 'intel_pstate' driver, got '$DRIVER'"
  exit 1
fi

# Set powersave governor
echo "==> Setting CPU governor to 'powersave'"
for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
  echo powersave | sudo tee "$cpu/cpufreq/scaling_governor" > /dev/null
done

# Find the lowest available frequency
if [ -f /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq ]; then
  LOWEST_FREQ=$(cat /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq)
  echo "==> Lowest frequency detected: $LOWEST_FREQ kHz"
else
  echo "Error: Could not find cpuinfo_min_freq"
  exit 1
fi

echo "==> Setting all CPUs to fixed frequency: $LOWEST_FREQ kHz"
for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
  echo $LOWEST_FREQ | sudo tee "$cpu/cpufreq/scaling_min_freq" > /dev/null
  echo $LOWEST_FREQ | sudo tee "$cpu/cpufreq/scaling_max_freq" > /dev/null
done

# Disable Turbo Boost
if [ -f /sys/devices/system/cpu/intel_pstate/no_turbo ]; then
  echo "==> Disabling Intel Turbo Boost"
  echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo > /dev/null
else
  echo "Warning: Turbo Boost control not available"
fi

# Disable C-states (except C0)
echo "==> Disabling all C-states except C0"
for state_file in /sys/devices/system/cpu/cpu*/cpuidle/state*/disable; do
  state=$(basename "$(dirname "$state_file")")
  if [[ "$state" != "state0" ]]; then
    echo 1 | sudo tee "$state_file" > /dev/null
  fi
done

# Verify C-state status
echo "==> Verifying that C-states are disabled:"
cpupower idle-info | awk '
/Analyzing CPU/ {cpu=$3}
/DISABLED/ && !disabled[cpu]++ { print cpu ": " $0 }
'

# Final CPU frequency check
echo "==> Final CPU frequencies:"
grep "cpu MHz" /proc/cpuinfo | sort | uniq

echo "==> Setup complete. CPU is locked to lowest frequency, Turbo Boost and C-states are disabled."
