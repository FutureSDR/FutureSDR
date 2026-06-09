#!/bin/bash

outfile=perf-data/results.csv
rm -f ${outfile}

echo "sdr,run,pipes,stages,samples,config,time" > ${outfile}

files=$(ls perf-data/gr_*.csv 2>/dev/null || echo)
for f in ${files}
do
	awk -F, 'NF == 6 { print "gr," $0 }' "$f" >> ${outfile}
done

files=$(ls perf-data/fs_*.csv 2>/dev/null || echo)
for f in ${files}
do
	awk -F, 'NF == 6 { print "fs," $0 }' "$f" >> ${outfile}
done
