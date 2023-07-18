#!/bin/bash
#
export RUSTFLAGS="-Clink-arg=-zstack-size=1888256 -Clink-arg=--import-memory --cfg=web_sys_unstable_apis  -Clink-arg=--initial-memory=33554432 -Clink-arg=--max-memory=4294967296"

trunk serve --release
