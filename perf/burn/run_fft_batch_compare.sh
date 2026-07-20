#!/usr/bin/env bash
set -euo pipefail

OUT="${1:-perf-data/fft_batch_sweep_wgpu.csv}"
RUNS="${RUNS:-5}"
BATCH_SIZES="${BATCH_SIZES:-2 4 8 16 32 64 128 256 512 1024 2048 4096}"
RESUME="${RESUME:-0}"
MAX_ATTEMPTS="${MAX_ATTEMPTS:-2}"
RUN_TIMEOUT="${RUN_TIMEOUT:-180s}"
FFT_SIZE=2048
MEASURED_BATCHES=$((1 << 10))
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST_PATH="${SCRIPT_DIR}/Cargo.toml"
SHIELD=(sudo systemd-run --uid="$(id -u)" --slice=sdr --property="RuntimeMaxSec=${RUN_TIMEOUT}" --wait -P -d)

bins=(
    fft-non-burn
    fft-wgpu-circular
    fft-wgpu-hack
)

if [[ -n "${BINS:-}" ]]; then
    read -r -a bins <<< "$BINS"
fi

mkdir -p "$(dirname "$OUT")"
header="run,bin,batch_size,samples,duration_raw,seconds"
if [[ "$RESUME" == "1" && -f "$OUT" ]]; then
    if [[ "$(head -n1 "$OUT")" != "$header" ]]; then
        echo "unexpected CSV header in ${OUT}" >&2
        exit 1
    fi
else
    echo "$header" > "$OUT"
fi

row_exists() {
    local run="$1"
    local bin="$2"
    local bs="$3"
    awk -F, -v run="$run" -v bin="$bin" -v bs="$bs" \
        '$1 == run && $2 == bin && $3 == bs { found = 1 } END { exit !found }' "$OUT"
}

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
            if [[ "$RESUME" == "1" ]] && row_exists "$run" "$bin" "$bs"; then
                echo "skip run=${run} bin=${bin} batch_size=${bs}"
                continue
            fi
            echo "run=${run} bin=${bin} batch_size=${bs}"
            success=0
            for attempt in $(seq 1 "$MAX_ATTEMPTS"); do
                log_file="$(mktemp)"
                command_ok=1
                if ! "${SHIELD[@]}" -- cargo run --release --manifest-path "$MANIFEST_PATH" --bin "$bin" -- --batch-size="$bs" \
                    2>&1 | tee "$log_file"
                then
                    command_ok=0
                fi
                dur="$(rg -o 'took [^ ]+' -N "$log_file" | tail -n1 | awk '{print $2}')"
                if [[ "$command_ok" == "1" && -n "${dur:-}" ]]; then
                    success=1
                    rm -f "$log_file"
                    break
                fi
                echo "attempt ${attempt}/${MAX_ATTEMPTS} failed for ${bin} batch_size=${bs}" >&2
                tail -n 50 "$log_file" >&2 || true
                rm -f "$log_file"
            done
            if [[ "$success" != "1" ]]; then
                echo "all attempts failed for ${bin} batch_size=${bs}" >&2
                exit 1
            fi
            sec="$(to_seconds "$dur")"
            measured_samples=$((bs * FFT_SIZE * MEASURED_BATCHES))
            echo "${run},${bin},${bs},${measured_samples},${dur},${sec}" >> "$OUT"
        done
    done
done

echo "wrote ${OUT}"
