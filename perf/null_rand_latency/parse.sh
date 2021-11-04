#!/bin/bash

outfile=perf-data/results.csv
rm -f ${outfile}

echo "sdr,run,pipes,stages,samples,max_copy,scheduler,time,event,cpu,block,samples" > ${outfile}

files=$(ls perf-data/gr_*.csv 2>/dev/null || echo)
for f in ${files}
do
	echo "gr,$(cat $f)" >> ${outfile}
done

files=$(ls perf-data/fs_*.csv 2>/dev/null || echo)
for f in ${files}
do
    data=(${f//\// })
    data=(${data[1]//_/ })
    prefix="${data[0]},${data[1]},${data[2]},${data[3]},${data[4]},${data[5]},${data[6]},"
    cat $f | sed -e "s/^/${prefix}/" >> ${outfile}
done
