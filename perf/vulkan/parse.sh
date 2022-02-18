#!/bin/bash

outfile=perf-data/results.csv
rm -f ${outfile}

echo "run,samples,buffer_size,time" > ${outfile}

files=$(ls perf-data/fs_*.csv 2>/dev/null || echo)
for f in ${files}
do
	echo "$(cat $f)" >> ${outfile}
done
