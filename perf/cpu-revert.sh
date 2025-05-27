#!/bin/bash

set -e

echo "==> Reverting CPU frequency and power settings to defaults"

# Detect CPU vendor
VENDOR=$(grep -m1 "vendor_id" /proc/cpuinfo | awk '{print $3}')
echo "Detected CPU vendor: $VENDOR"

# Check cpufreq driver
DRIVER=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_driver)
echo "Detected cpufreq driver: $DRIVER"

# Choose best available governor
AVAILABLE=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_available_governors)
if [[ "$AVAILABLE" == *"schedutil"* ]]; then
  GOV="schedutil"
elif [[ "$AVAILABLE" == *"ondemand"* ]]; then
  GOV="ondemand"
else
  GOV="performance"
fi

echo "==> Setting CPU governor to '$GOV'"
for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
  echo $GOV | sudo tee "$cpu/cpufreq/scaling_governor" > /dev/null
done

# Restore full frequency range
if [ -f /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq ] && \
   [ -f /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq ]; then
  MIN=$(cat /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_min_freq)
  MAX=$(cat /sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq)

  echo "==> Resetting frequency range to $MIN – $MAX"
  for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
      echo $MIN | sudo tee "$cpu/cpufreq/scaling_min_freq" > /dev/null
      echo $MAX | sudo tee "$cpu/cpufreq/scaling_max_freq" > /dev/null
  done
else
  echo "Warning: Unable to detect cpuinfo_min/max_freq — skipping frequency reset"
fi

# Re-enable Turbo Boost
if [ -f /sys/devices/system/cpu/intel_pstate/no_turbo ]; then
  echo "==> Re-enabling Intel Turbo Boost"
  echo 0 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo > /dev/null
elif [ -f /sys/devices/system/cpu/cpufreq/boost ]; then
  echo "==> Re-enabling AMD Turbo (CPB/boost)"
  echo 1 | sudo tee /sys/devices/system/cpu/cpufreq/boost > /dev/null
else
  echo "==> No turbo control interface available"
fi

# Re-enable C-states
echo "==> Re-enabling all CPU C-states"
for state_file in /sys/devices/system/cpu/cpu*/cpuidle/state*/disable; do
  echo 0 | sudo tee "$state_file" > /dev/null
done

# Show summary
echo "==> Current CPU governors:"
for cpu in /sys/devices/system/cpu/cpu[0-9]*; do
  echo -n "$(basename $cpu): "
  cat "$cpu/cpufreq/scaling_governor"
done

echo "==> CPU frequencies:"
grep "cpu MHz" /proc/cpuinfo | sort | uniq

echo "==> Verifying C-state availability:"
cpupower idle-info | grep DISABLED || echo "All C-states enabled"

echo "==> Reversion complete."
