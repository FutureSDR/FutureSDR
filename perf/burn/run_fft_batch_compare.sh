#!/usr/bin/env bash
set -euo pipefail

OUT="${1:-perf-data/fft_batch_compare.csv}"
RUNS="${RUNS:-5}"
BATCH_SIZES="${BATCH_SIZES:-512 1024 2048 4096 8000 12000}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST_PATH="${SCRIPT_DIR}/Cargo.toml"

bins=("fft-non-burn" "fft-wgpu-hack")

mkdir -p "$(dirname "$OUT")"
echo "run,bin,batch_size,duration_raw,seconds" > "$OUT"

to_seconds() {
    local d="$1"
    case "$d" in
    *ns) awk -v v="${d%ns}" 'BEGIN { printf "%.12f", v/1e9 }' ;;
    *us) awk -v v="${d%us}" 'BEGIN { printf "%.12f", v/1e6 }' ;;
    *µs) awk -v v="${d%µs}" 'BEGIN { printf "%.12f", v/1e6 }' ;;
    *μs) awk -v v="${d%μs}" 'BEGIN { printf "%.12f", v/1e6 }' ;;
    *ms) awk -v v="${d%ms}" 'BEGIN { printf "%.12f", v/1e3 }' ;;
    *s) awk -v v="${d%s}" 'BEGIN { printf "%.12f", v }' ;;
    *) echo "nan" ;;
    esac
}

for run in $(seq 0 $((RUNS - 1))); do
    for bs in $BATCH_SIZES; do
        for bin in "${bins[@]}"; do
            echo "run=${run} bin=${bin} batch_size=${bs}"
            log_file="$(mktemp)"
            if ! cargo run --release --manifest-path "$MANIFEST_PATH" --bin "$bin" -- --batch-size="$bs" \
                2>&1 | tee "$log_file"
            then
                echo "command failed for ${bin} batch_size=${bs}" >&2
                tail -n 50 "$log_file" >&2 || true
                rm -f "$log_file"
                exit 1
            fi
            dur="$(rg -o 'took [^ ]+' -N "$log_file" | tail -n1 | awk '{print $2}')"
            if [[ -z "${dur:-}" ]]; then
                echo "failed to parse duration for ${bin} batch_size=${bs}" >&2
                tail -n 50 "$log_file" >&2 || true
                rm -f "$log_file"
                exit 1
            fi
            sec="$(to_seconds "$dur")"
            echo "${run},${bin},${bs},${dur},${sec}" >> "$OUT"
            rm -f "$log_file"
        done
    done
done

echo "wrote ${OUT}"
