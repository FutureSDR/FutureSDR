.PHONY: build

build:
	RUSTFLAGS='-C target-cpu=generic --cfg=web_sys_unstable_apis' wasm-pack build --target web --out-name wasm --out-dir ./dist --dev
	./gulp default serve

