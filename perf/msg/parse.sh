#!/bin/bash

outfile=perf-data/results.csv
rm -f ${outfile}

echo "sdr,run,pipes,stages,repetition,burst_size,time" > ${outfile}

files=$(ls perf-data/gr_*.csv 2>/dev/null || echo)
for f in ${files}
do
    awk '$0="gr,"$0' $f >> ${outfile}
done

files=$(ls perf-data/nr_*.csv 2>/dev/null || echo)
for f in ${files}
do
	echo "nr,$(cat $f)" >> ${outfile}
done
