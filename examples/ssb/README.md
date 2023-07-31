SSB
===

Examples of an SSB modulator (`transmit`) and demodulator (`receive`).

## Usage

### Receive

```
cargo run --release --bin receive
```

By default, this reads the file `ssb_lsb_256k_complex2.dat` and outputs the audio to your default audio device.

You can download that file from https://www.csun.edu/~skatz/katzpage/sdr_project/sdr/ssb_lsb_256k_complex2.dat.zip or by running `make ssb_lsb_256k_complex2.dat`

![](flowgraph-2022-07-28-124646.png)

### Transmit

```
cargo run --release --bin transmit INPUT OUTPUT
```

* `INPUT` must be an audio file with only one chanel (mono).
* `OUTPUT` name of the output without file extension.

Two output files are generated.
A wave file with 16 bit integer values with I on the right and Q on the left chanel.
A dat file with 32 bit complex floating point values.
You can view/listen to the wav file using a software define radio application like [SDR++](https://www.sdrpp.org/).
The dat file can be listened to using the `receive` binary (see above) or the [GNURadio flowgraph](./ssb-decoder.grc).

## Architecture

Goals is to achieve SSB decoding as in:
* https://wiki.gnuradio.org/index.php/Simulation_example:_Single_Sideband_transceiver
* http://www.csun.edu/~skatz/katzpage/sdr_project/sdr/grc_tutorial4.pdf

So really have same result as this [GNURadio flowgraph](./ssb-decoder.grc).
