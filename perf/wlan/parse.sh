#!/bin/bash
set -euo pipefail

outfile=perf-data/results.csv
mkdir -p perf-data
rm -f "${outfile}"

echo "sdr,run,file,config,time,frames" > "${outfile}"

for f in perf-data/gr_*.csv; do
    [[ -e "$f" ]] || continue
    line=$(tail -n 1 "$f")
    run=$(echo "$line" | cut -d, -f1)
    file=$(echo "$line" | cut -d, -f2)
    time=$(echo "$line" | cut -d, -f3)
    frames=$(echo "$file" | sed -E 's/.*wlan-([0-9]+)\.cf32/\1/')
    echo "gr,${run},${file},legacy,${time},${frames}" >> "${outfile}"
done

for f in perf-data/fs_*.csv; do
    [[ -e "$f" ]] || continue
    line=$(tail -n 1 "$f")
    run=$(echo "$line" | cut -d, -f1)
    file=$(echo "$line" | cut -d, -f2)
    config=$(echo "$line" | cut -d, -f3)
    time=$(echo "$line" | cut -d, -f4)
    frames=$(echo "$file" | sed -E 's/.*wlan-([0-9]+)\.cf32/\1/')
    echo "fs,${run},${file},${config},${time},${frames}" >> "${outfile}"
done
