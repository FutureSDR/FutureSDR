#!/bin/bash

outfile=perf-data/results.csv
rm -f ${outfile}

echo "sdr,run,pipes,stages,repetition,burst_size,config,time" > ${outfile}

files=$(ls perf-data/gr_*.csv 2>/dev/null || echo)
for f in ${files}
do
  awk '$0="gr,"$0' $f >> ${outfile}
done

files=$(ls perf-data/fs_*.csv 2>/dev/null || echo)
for f in ${files}
do
  awk '$0="fs,"$0' $f >> ${outfile}
done
