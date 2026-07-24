#!/bin/bash
set -euo pipefail

outfile=perf-data/results.csv
mkdir -p perf-data
rm -f "${outfile}"

echo "sdr,run,file,payload,config,time,branch0,branch1,branch2,branch3,expected_branch0,expected_branch1,expected_branch2,expected_branch3" > "${outfile}"

payload_from_file() {
    sed -E 's/.*_([0-9]+)B_.*/\1/' <<< "$1"
}

expected_counts() {
    python3 - "$1" <<'PY'
import ast
import sys

try:
    with open(sys.argv[1] + ".txt", "r", encoding="utf-8") as f:
        counts = ast.literal_eval(f.read().strip())
    sf7_index = 2 if len(counts) == 8 else 0
    expected = counts[sf7_index]
    print(f"{expected},{expected},{expected},{expected}")
except Exception:
    print(",,,")
PY
}

append_result() {
    local sdr=$1
    local f=$2
    local line run file config time branch0 branch1 branch2 branch3 payload expected

    line=$(tail -n 1 "$f")
    IFS=, read -r run file config time branch0 branch1 branch2 branch3 <<< "$line"
    payload=$(payload_from_file "$file")
    expected=$(expected_counts "$file")
    echo "${sdr},${run},${file},${payload},${config},${time},${branch0},${branch1},${branch2},${branch3},${expected}" >> "${outfile}"
}

for f in perf-data/gr_*.csv; do
    [[ -e "$f" ]] || continue
    append_result gr "$f"
done

for f in perf-data/fs_*.csv; do
    [[ -e "$f" ]] || continue
    append_result fs "$f"
done
