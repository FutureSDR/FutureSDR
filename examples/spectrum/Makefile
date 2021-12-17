.PHONY: cpu vulkan wasm serve

wasm:
	RUSTFLAGS='-C target-cpu=generic' wasm-pack build --target web --out-name spectrum --out-dir ./dist --dev
	cd ../../frontend && make
	./gulp
	cd dist && ../serve.py

serve:
	cd dist && ../serve.py

cpu:
	cargo run --bin=cpu --features=cpu

vulkan:
	cargo run --bin=vulkan --features=cpu,vulkan

