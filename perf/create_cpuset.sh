#!/bin/bash

sudo systemctl set-property --runtime -- user.slice AllowedCPUs=0-3
sudo systemctl set-property --runtime -- system.slice AllowedCPUs=0-3
sudo systemctl set-property --runtime -- init.scope AllowedCPUs=0-3
