.PHONY: build

build:
	RUSTFLAGS='-C target-cpu=generic' wasm-pack build --target web --out-name futuresdr --out-dir ./dist --release
	./gulp
