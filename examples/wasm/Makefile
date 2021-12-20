.PHONY: build

build:
	RUSTFLAGS='-C target-cpu=generic' wasm-pack build --target web --out-name wasm --out-dir ./dist --dev
	./gulp
	cd dist && ../server.py

