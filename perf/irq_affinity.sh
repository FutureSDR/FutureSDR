#!/bin/bash

sudo systemctl stop irqbalance.service

for i in $(ls /proc/irq/*/smp_affinity)
do
    echo  33 | sudo tee $i
done
