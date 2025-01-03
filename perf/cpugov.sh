#!/bin/bash

set -e

if [ "$#" -lt "1" ]
then
    GOV=performance
else
    GOV=$1
fi

sudo cpupower frequency-set -g $GOV

