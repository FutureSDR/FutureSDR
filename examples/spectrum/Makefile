.PHONY: build cpu vulkan

wasm:
	RUSTFLAGS='-C target-cpu=generic' wasm-pack build --target web --out-name wasm --out-dir ./dist --dev

cpu:
	cargo run --bin=cpu --features=cpu

vulkan:
	cargo run --bin=vulkan --features=cpu,vulkan

