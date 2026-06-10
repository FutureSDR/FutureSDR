#!/bin/bash

outfile=perf-data/results.csv
rm -f ${outfile}

echo "run,pipes,stages,samples,chunk,buffer_size,config,buffer,time" > ${outfile}

files=$(ls perf-data/fs_*.csv 2>/dev/null || echo)
for f in ${files}
do
	if [ -s "$f" ]; then
		cat "$f" >> ${outfile}
	fi
done
