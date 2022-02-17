.PHONY: wasm serve

wasm:
	wasm-pack build --target web --out-name cw --out-dir ./dist --dev

serve: wasm
	./gulp
	cd dist && ../serve.py

