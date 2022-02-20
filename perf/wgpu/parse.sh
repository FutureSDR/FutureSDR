#!/bin/bash

outfile=perf-data/results.csv
rm -f ${outfile}

echo "run,scheduler,samples,buffer_size,time" > ${outfile}

files=$(ls perf-data/fs_*.csv 2>/dev/null || echo)
for f in ${files}
do
	echo "$(cat $f)" >> ${outfile}
done

files=$(ls perf-data/*.log 2>/dev/null || echo)
for f in ${files}
do
    cut -d " " -f 2 ${f} >> ${outfile}
done
