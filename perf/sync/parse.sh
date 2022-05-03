#!/bin/bash

outfile=perf-data/results.csv
rm -f ${outfile}

echo "run,pipes,stages,samples,max_copy,scheduler,buffer,sync,time" > ${outfile}

files=$(ls perf-data/fs_*.csv 2>/dev/null || echo)
for f in ${files}
do
	echo "$(cat $f)" >> ${outfile}
done
