#!/bin/bash

sudo systemctl stop irqbalance.service

for i in $(ls /proc/irq/*/smp_affinity)
do
    echo  e38 | sudo tee $i
done
