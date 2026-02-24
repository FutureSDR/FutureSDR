# Running Flowgraphs

## Configuration

FutureSDR offers runtime options that can be configured through a `config.toml` or environment variables.

It will search for a global user config at `~/.config/futuresdr/config.toml`, a
project `config.toml` in the current directory, and environment variables. The
user config has the lowest precedence, while environment variables have the
highest precedence.

The available options are:

- `queue_size`: number of messages that fit into a block’s inbox
- `buffer_size`: default minimum size of a stream buffer in bytes
- `stack_size`: stack size (in bytes) for all threads
- `slab_reserved`: number of items a Slab buffer copies into the next buffer
- `log_level`: one of `off`, `info`, `warn`, `error`, `debug`, or `trace`
- `ctrlport_enable`: whether control port should be enabled (`true` or
  `false`)
- `ctrlport_bind`: endpoint that the control-port web server should bind to
  (e.g., `127.0.0.1:1337`)
- `frontend_path`: path to a web UI that is served as the root URL of the
  control-port server

An example `config.toml`:
```toml
log_level = "debug"
buffer_size = 32768
queue_size = 8192
ctrlport_enable = true
ctrlport_bind = "127.0.0.1:1337"
```

Alternatively, pass these options through environment variables. Each key uses
the prefix `FUTURESDR_` and is uppercased:

```bash
export FUTURESDR_CTRLPORT_ENABLE="true"
export FUTURESDR_CTRLPORT_BIND="0.0.0.0:1337"
```

## Rust Features

Some examples use Cargo features to selectively enable functionality such as SDR
drivers or GPU backends. Check the `[features]` section in an example’s
`Cargo.toml` for the full list of supported flags.

```toml
[features]
default = ["soapy"]
aaronia_http = ["futuresdr/aaronia_http"]
soapy = ["futuresdr/soapy"]
```

In this example `soapy` is enabled by default, and the Aaronia HTTP driver can
be enabled by adding the corresponding feature.

```bash
cargo run --release --bin rx --features=aaronia_http
```

Disable default features with:

```bash
cargo run --release --bin rx --no-default-features
```


## Log and Debug Messages

FutureSDR uses the [`tracing`](https://docs.rs/tracing/) library for log and debug messages.
Applications can set their own handler for log messages, otherwise FutureSDR will
set [`EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) as default handler.
If the application uses a custom handler, the logging-related configuration of
FutureSDR will not be considered, and you'd have to check with the documentation
of the application for information about logging.

If no log handler is set when a flowgraph is launched on a runtime, FutureSDR will
set `EnvFilter`. There are extensive configuration options to configure logging per
module through environment variables.
Please see the [documentation](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html).

Some examples:

```bash
# set log level to warn
FUTURESDR_LOG=warn cargo run --bin rx

# disable log messages from lora::frame_sync module
FUTURESDR_LOG=lora::frame_sync=off cargo run --bin rx

# set default log level to info but disable messages from lora::decoder
FUTURESDR_LOG=info,lora::decoder=off cargo run --release --bin rx
```


> [!WARNING]
> By default, FutureSDR sets feature flags that disable `tracing` level log messages in debug mode and everything more detailed than `info` in release mode. This is a *compile time* filter!
>
> Also, these flags are transitive! If you want more detailed logs in your application, disable default features for the FutureSDR dependency.
> ```rustc
> [dependencies]
> futuresdr = { version = ..., default-features=false, features = ["foo", "bar"] }
> ```

## Command Line Arguments

Most examples allow passing command line arguments.
When running the application with `cargo`, use `--` to separate Cargo’s
arguments from the application’s arguments.

To check which arguments are available, pass the `-h/--help` flag.

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

> [!IMPORTANT]
> When running applications with `cargo`, use `--` to separate command line parameters of cargo and the application.
>
> ```bash
> cargo run --release --bin foo -- --sample_rate 3e6
> ```

## SDR Device Selection and Configuration

Most example applications support an `-a/--argument` command line option that is passed to the SDR hardware drivers.
The argument can be used to pass additional options, select the hardware driver, or specify the SDR, if more than one is connected.

Driver selection can be necessary in more cases than one might expect.
FutureSDR uses [Seify](https://github.com/futuresdr/seify) as SDR hardware abstraction layer, which usually defaults to using [Soapy](https://github.com/pothosware/SoapySDR) drivers under the hood.
Many distributions ship a bundle of Soapy drivers that include an audio driver, which enumerates your sound card as SDR.
You can run `SoapySDR --probe` to see what is detected.

If Seify selects the wrong device, specify the device argument to select the
correct one by defining the driver (e.g., `-a soapy_driver=rtlsdr`) and
optionally the device index (e.g., `-a soapy_driver=rtlsdr,index=1`) or any
other identifier supported by the driver (e.g., serial number, IP address, or
USB device ID).
See the driver documentation for information about what is supported.

A complete command could be

```bash
cargo run --release --bin receiver -- -a soapy_driver=rtlsdr
```

> [!IMPORTANT]
> Seify will forward all arguments to Soapy. Only the `driver` argument has to be prefixed to `soapy_driver` to differentiate it from Seify driver selection.
> ```

> [!IMPORTANT]
> Soapy might select the wrong device even if only one SDR is plugged into your PC.
> Use the `-a/--argument` to select the Soapy driver, e.g., `-a soapy_driver=rtlsdr`.
> ```

