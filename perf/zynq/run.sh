#!/bin/bash

set -xe

RUNS=$(seq 0 9)
MAX_COPYS="$((2 ** 6)) $((2 ** 7)) $((2 ** 8)) $((2 ** 9)) $((2 ** 10)) $((2 ** 11)) $((2 ** 12)) $((2 ** 13)) $((2 ** 14)) $((2 ** 15)) $((2 ** 16)) $((2 ** 18)) $((2 ** 20)) $((2 ** 22))"
ITEMS=$((2 ** 24))
OUTFILE="data.csv"

echo "run,items,max_copy,sync,time" > ${OUTFILE}

for R in ${RUNS}
do
    for M in $MAX_COPYS
    do
        ./zynq -n ${ITEMS} -m ${M} -r ${R} >> ${OUTFILE}
    done
done

for R in ${RUNS}
do
    for M in $MAX_COPYS
    do
        ./zynq -n ${ITEMS} -m ${M} -r ${R} -s >> ${OUTFILE}
    done
done
