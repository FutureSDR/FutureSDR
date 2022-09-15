run: ssb_lsb_256k_complex2.dat
	cargo run --release

ssb_lsb_256k_complex2.dat.zip:
	wget https://www.csun.edu/~skatz/katzpage/sdr_project/sdr/ssb_lsb_256k_complex2.dat.zip

ssb_lsb_256k_complex2.dat: ssb_lsb_256k_complex2.dat.zip
	unzip -DD $<

.PHONY: clean
clean:
	rm -f ssb_lsb_256k_complex2.dat ssb_lsb_256k_complex2.dat.zip
