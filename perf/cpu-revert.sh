#!/bin/bash

set -e

echo "==> Reverting CPU frequency and power settings to defaults"

# Check driver
DRIVER=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_driver)
if [[ "$DRIVER" != "intel_pstate" ]]; then
  echo "Warning: Expected 'intel_pstate' driver, got '$DRIVER'. Proceeding anyway."
fi

# Set governor back to 'schedutil' if available, else 'performance'
AVAILABLE=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors)
if [[ "$AVAILABLE" == *"schedutil"* ]]; then
  GOV="schedutil"
else
  GOV="performance"
fi

echo "==> Setting CPU governor to '$GOV'"
for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
  echo $GOV | sudo tee "$cpu/cpufreq/scaling_governor" > /dev/null
done

# Restore min/max frequencies to hardware-defined defaults
if [ -f /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq ] && \
   [ -f /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq ]; then
  MIN=$(cat /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq)
  MAX=$(cat /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq)

  echo "==> Restoring frequency range: $MIN – $MAX kHz"
  for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
    echo $MIN | sudo tee "$cpu/cpufreq/scaling_min_freq" > /dev/null
    echo $MAX | sudo tee "$cpu/cpufreq/scaling_max_freq" > /dev/null
  done
fi

# Re-enable Turbo Boost
if [ -f /sys/devices/system/cpu/intel_pstate/no_turbo ]; then
  echo "==> Re-enabling Intel Turbo Boost"
  echo 0 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo > /dev/null
fi

# Re-enable all C-states
echo "==> Re-enabling all CPU C-states"
for state_file in /sys/devices/system/cpu/cpu*/cpuidle/state*/disable; do
  echo 0 | sudo tee "$state_file" > /dev/null
done

# Show current state
echo "==> Current governor and frequency:"
for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
  echo -n "$(basename $cpu): "
  cat "$cpu/cpufreq/scaling_governor"
done

grep "cpu MHz" /proc/cpuinfo | sort | uniq

echo "==> C-state availability:"
cpupower idle-info | grep DISABLED || echo "All C-states enabled"

echo "==> Reversion complete."
