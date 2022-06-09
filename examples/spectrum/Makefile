.PHONY: cpu vulkan wasm serve

wasm:
	RUSTFLAGS='-C target-cpu=generic --cfg=web_sys_unstable_apis' wasm-pack build --target web --out-name spectrum --out-dir ./dist --release #--dev
	cd ../../frontend && make && ./gulp
	./gulp
	cd dist && ../serve.py

serve:
	cd dist && ../serve.py

cpu:
	cargo run --bin=cpu --features=cpu

vulkan:
	cargo run --bin=vulkan --features=cpu,vulkan

