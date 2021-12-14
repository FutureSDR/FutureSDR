.PHONY: build cpu

build:
	wasm-pack build --target web --out-name wasm --out-dir ./dist --dev

cpu:
	cargo run --bin=cpu --features=cpu

