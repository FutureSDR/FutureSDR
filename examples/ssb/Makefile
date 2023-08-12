run: ssb_lsb_256k_complex2.dat
	cargo run --release --bin receive

ssb_lsb_256k_complex2.dat.zip:
	wget https://www.csun.edu/~skatz/katzpage/sdr_project/sdr/ssb_lsb_256k_complex2.dat.zip

ssb_lsb_256k_complex2.dat: ssb_lsb_256k_complex2.dat.zip
	unzip -DD $<

transmitted.%: test.wav
	cargo run --release --bin transmit -- $< $(subst .$*,,$@)

.PHONY: roundtrip
rountrip: transmitted.dat
	cargo run --release --bin receive -- -f $<

.PHONY: clean
clean:
	rm -f transmitted.*

.PHONY: clean_all
clean_all: clean
	rm -f ssb_lsb_256k_complex2.dat ssb_lsb_256k_complex2.dat.zip
