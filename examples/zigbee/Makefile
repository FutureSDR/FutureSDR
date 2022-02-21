SHELL=/bin/bash

.PHONY: wasm serve

wasm:
	cd ${HOME}/src/webusb-libusb && . ${HOME}/src/emsdk/emsdk_env.sh && make hackrf_open
	cp ${HOME}/src/webusb-libusb/build/example/hack* ${HOME}/src/futuresdr/examples/zigbee/assets/static/
	wasm-pack build --target web --out-name zigbee --out-dir ./dist --profiling #--dev
	./gulp
	cd dist && ../serve.py
