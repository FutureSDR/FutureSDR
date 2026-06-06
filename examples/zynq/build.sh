#!/bin/bash

set -e

cd "$(dirname "$0")"
cross build --target=aarch64-unknown-linux-gnu
