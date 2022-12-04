#!/bin/bash

echo ### deleting CPU Sets
sudo cset -m set | grep ";" | grep -v root | cut -d ";" -f1 | xargs -n1 sudo cset set --destroy 

echo ### Creating new CPU Sets
sudo cset shield --sysset=system --userset=sdr --cpu=0,1,2,3,8,9,10,11 --kthread=on

echo ### CPU Sets
sudo cset set

sudo chown -R root:basti /sys/fs/cgroup/cpuset
sudo chmod -R g+rwx /sys/fs/cgroup/cpuset
