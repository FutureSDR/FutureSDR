.PHONY: cpu vulkan wasm

wasm:
	RUSTFLAGS='-C target-cpu=generic' wasm-pack build --target web --out-name futuresdr --out-dir ./dist --dev
	./gulp
	cd dist && ../serve.py

cpu:
	cargo run --bin=cpu --features=cpu

vulkan:
	cargo run --bin=vulkan --features=cpu,vulkan

