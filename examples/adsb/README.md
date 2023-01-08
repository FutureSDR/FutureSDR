# ADS-B Decoder

To start, compile and run ADS-B decoder in one terminal:
```
cargo run --release
```
Then start the web server from another terminal:
```
npm install
./gulp
cd dist && python ../serve.py
```
The map is served at http://localhost:8000.
