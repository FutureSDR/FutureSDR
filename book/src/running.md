# Running Flowgraphs

## FutureSDR Runtime Configuration

## Debug Output


## Command Line Arguments

Many examples support command line arguments.
When running the application with `cargo`, use `--` to separate command line parameters of Cargo and the application.

To check which arguments are available pass the `-h/--help` flag.

```txt
$ cargo run --release -- -h
Usage: fm-receiver [OPTIONS]

Options:
  -g, --gain <GAIN>              Gain to apply to the seify source [default: 30]
  -f, --frequency <FREQUENCY>    Center frequency [default: 100000000]
  -r, --rate <RATE>              Sample rate [default: 1000000]
  -a, --args <ARGS>              Seify args [default: ]
      --audio-mult <AUDIO_MULT>  Multiplier for intermedia sample rate
      --audio-rate <AUDIO_RATE>  Audio Rate
  -h, --help                     Print help

```

~~~admonish important
When running applications with `cargo`, use `--` to separate command line parameters of cargo and the application.

```bash
cargo run --release --bin foo -- --sample_rate 3e6
```
~~~

## SDR Device Selection and Configuration

Most example applications support an `-a/--argument` command line option that is passed to the SDR hardware drivers.
The argument can be used to pass additional options, select the hardware driver, or specify the SDR, if more than one is connected.

Driver selection can be necessary in more cases than one might expect.
FutureSDR uses [Seify](https://github.com/futuresdr/seify) as SDR hardware abstraction layer, which usually defaults to using [Soapy](https://github.com/pothosware/SoapySDR) drivers under the hood.
Many distributions ship a bundle of Soapy drivers that include an audio driver, which enumerates your sound card as SDR.
You can run `SoapySDR --probe` to see what is detected.

If Seify selects the wrong device, specify the device argument to select the correct one by defining the driver (e.g. `-a soapy_driver=rtlsdr`) and potentially also the device index (e.g., `-a soapy_driver=rtsdr,index=1`) or any other identifier supported by the driver (e.g., serial number, IP, USB device ID).
See the driver documentation for information about what is supported.

A complete command could be

```bash
cargo run --release --bin receiver -- -a soapy_driver=rtlsdr
```

```admonish important
Seify will forward all arguments to Soapy. Only the `driver` argument has to be prefixed to `soapy_driver` to differentiate it from Seify driver selection.
```

```admonish important
Soapy might select the wrong device even if only one SDR is plugged into your PC.
Use the `-a/--argument` to select the Soapy driver, e.g., `-a soapy_driver=rtlsdr`.
```
