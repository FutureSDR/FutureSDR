#!/bin/bash

wasm-pack build --release --target web --no-typescript --weak-refs --reference-types --out-dir assets --out-name wasm-decoder --no-pack
