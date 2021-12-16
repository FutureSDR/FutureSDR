.PHONY: build

build:
	RUSTFLAGS='-C target-cpu=generic --cfg=web_sys_unstable_apis' cargo build --lib --no-default-features --target wasm32-unknown-unknown
	wasm-bindgen --out-dir ./dist --web ../../target/wasm32-unknown-unknown/debug/wasm.wasm
