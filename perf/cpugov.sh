#!/bin/bash

set -e

if [ "$#" -lt "1" ]
then
    GOV=performance
else
    GOV=$1
fi

CORES=$(getconf _NPROCESSORS_ONLN)
i=0

echo "New CPU governor: ${GOV}"

while [ "$i" -lt "$CORES" ]
do
    sudo cpufreq-set -c $i -g $GOV
    i=$(( $i+1 ))
done

