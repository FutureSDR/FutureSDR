# Running Flowgraphs

## Configuration

FutureSDR offers runtime options that can be configured through a `config.toml` or environment variables.

It will search for a global user config at `~/.config/futuresdr/config.toml`, a project `config.toml` in the local directory and environment variables.
The user config has the lowest precedence, while the environment variables have the highest precedence.

The available options are:

- `queue_size`: the number of messages that fit into the inbox of blocks
- `buffer_size`: the default minimum size of a stream buffer in bytes
- `stack_size`: the stack size
- `slab_reserved`: the default number of items a Slab buffer will copy over to the next buffer
- `log_level`: one of `off`, `info`, `warn`, `error`, `debug`, `trace`
- `ctrlport_enable`: shout control port be enabled (`true` or `false`)
- `ctrlport_bind`: the endpoint that the web server for control port should bind to (e.g., `127.0.0.1:1337`)
- `frontend_path`: the path to a web UI that will be served as the root URL of the control port server

An example `config.toml`:
```toml
log_level = "debug"
buffer_size = 32768
queue_size = 8192
ctrlport_enable = true
ctrlport_bind = "127.0.0.1:1337"
```

Alternatively, these options can be passed through environment variables, e.g.:

```bash
export FUTURESDR_ctrlport_enable="true"
export FUTURESDR_ctrlport_bind="0.0.0.0:1337"
```

## Rust Features

Some examples use Rust features to selectively enable functionality like SDR drivers or GPU backends.
See the `[features]` section in the `Cargo.toml` of the example for a list of supported features.

```toml
[features]
default = ["soapy"]
aaronia_http = ["futuresdr/aaronia_http"]
soapy = ["futuresdr/soapy"]
```

In this example `soapy` is enabled by default and the Aaronia HTTP driver can be enabled by enabling the corresponding feature.

```bash
cargo run --release --bin rx --features=aaronia_http
```

Default features can be disabled with

```bash
cargo run --release --bin rx --no-default-features
```


## Log and Debug Messages

FutureSDR uses the [`tracing`](https://docs.rs/tracing/) library for log and debug messages.

~~~admonish warning
By default, FutureSDR sets feature flags that disable `tracing` level log messages in debug mode and everything more detailed than `info` in release mode. This is a *compile time* filter!

Also, these flags are transitive! If you want more detailed logs in your application, disable default features for the FutureSDR dependency.
```rustc
[dependencies]
futuresdr = { version = ..., default-features=false, features = ["foo", "bar"] }
```
~~~

## Command Line Arguments

Most examples allow passing command line arguments.
When running the application with `cargo`, use `--` to separate command line arguments of Cargo and the application.

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


## REST API

*Control port* provides a REST API to expose the flowgraph structure and enable remote interaction.
It is enabled by default, but can be configured explicitly through the [configuration](#configuration), e.g.:

```toml
ctrlport_enable = true
ctrlport_bind = "127.0.0.1:1337"
```

If you want to allow remote hosts to access control port, you can bind it to a network or allow access from arbitrary networks

```toml
ctrlport_enable = true
ctrlport_bind = "0.0.0.0:1337"
```

Alternatively, these options can be passed through environment variables, which always take precedence.

```bash
export FUTURESDR_ctrlport_enable="true"
export FUTURESDR_ctrlport_bind="0.0.0.0:1337"
```

Control port can be accessed with the browser or programmatically (e.g., using Curl, Python `requests` library, etc.).
FutureSDR also provides a [support library](https://crates.io/crates/futuresdr-remote) to ease remote interaction from Rust.

To get a JSON description of the first flowgraph that is executed on a runtime, you can visit `127.0.0.1:1337/api/fg/0/` with your browser or use Curl to query it.

```bash
curl http://127.0.0.1:1337/api/fg/0/ | jq
{
  "blocks": [
    {
      "id": 0,
      "type_name": "Encoder",
      "instance_name": "Encoder-0",
      "stream_inputs": [],
      "stream_outputs": [
        "output"
      ],
      "message_inputs": [
        "tx"
      ],
      "message_outputs": [],
      "blocking": false
    },
    {
      "id": 1,
      "type_name": "Mac",
      "instance_name": "Mac-1",
      "stream_inputs": [],
      "stream_outputs": [],
      "message_inputs": [
        "tx"
      ],
      "message_outputs": [
        "tx"
      ],
      "blocking": false
    },
  ],
  "stream_edges": [
    [
      0,
      "output",
      2,
      "input"
    ],
  ],
  "message_edges": [
    [
      1,
      "tx",
      0,
      "tx"
    ],
  ]
}
```

It is also possible to get information about a particular block.

```bash
curl http://127.0.0.1:1337/api/fg/0/block/0/ | jq
{
  "id": 0,
  "type_name": "Encoder",
  "instance_name": "Encoder-0",
  "stream_inputs": [],
  "stream_outputs": [
    "output"
  ],
  "message_inputs": [
    "tx"
  ],
  "message_outputs": [],
  "blocking": false
}
```

All message handlers of a block are exposed automatically through the REST API.
Assuming block `0` is the SDR source or sink, one can set the frequency by posting a JSON serialized PMT to the corresponding message handler.

```bash
curl -X POST -H "Content-Type: application/json" -d '{ "U32": 123 }'  http://127.0.0.1:1337/api/fg/0/block/0/call/freq/
```

Here are some more examples for serialized PMTs.

```json
{ "U32": 123 }
{ "U64": 5}
{ "F32": 123 }
{ "Bool": true }
{ "VecU64": [ 1, 2, 3] }
"Ok"
"Null"
{ "String": "foo" }
```


## Web UI

FutureSDR comes with a very minimal, work-in-progress Web UI, implemented in the *prophecy* crate.
It comes pre-compiled at `crates/prophecy/dist`.
When FutureSDR is started with control port enabled, one can specify the `frontend_path` [configuration](#configuration) option to set the path to the frontend, which will be served at the root path the control port URL, e.g., `127.0.0.1:1337`.

Using the REST API, it is straightforward to build custom UIs.

- A web UI served by an independent server
- A web UI served through FutureSDR control port (see the WLAN and ADS-B examples)
- A UI using arbitrary technology (GTK, QT, etc.) running as separate process (see the Egui example)


