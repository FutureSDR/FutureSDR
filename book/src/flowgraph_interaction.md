# Flowgraph Interaction

It is possible to interact with a running flowgraph through the control port REST API, which
can be used as the base for arbitrary UIs using web technology or any other GUI framework.

## REST API

*Control port* provides a REST API to expose the flowgraph structure and enable remote interaction.
It is enabled by default, but you can configure it explicitly through the
[configuration](./running.md#configuration), for example:

```toml
ctrlport_enable = true
ctrlport_bind = "127.0.0.1:1337"
```

To allow remote hosts to access control port, bind it to a public interface or
an unrestricted address:

```toml
ctrlport_enable = true
ctrlport_bind = "0.0.0.0:1337"
```

Alternatively, configure control port through environment variables, which
always take precedence:

```bash
export FUTURESDR_CTRLPORT_ENABLE="true"
export FUTURESDR_CTRLPORT_BIND="0.0.0.0:1337"
```

Control port can be accessed with a browser or programmatically (e.g., using
`curl`, the Python `requests` library, etc.).
FutureSDR also provides a [support library](https://crates.io/crates/futuresdr-remote) to ease remote interaction from Rust.

To get a JSON description of the first flowgraph executed on a runtime, open
`127.0.0.1:1337/api/fg/0/` in your browser or use `curl`:

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
Assuming block `0` is the SDR source or sink, you can set the frequency by
posting a JSON-serialized [PMT](https://docs.rs/futuresdr-types/latest/futuresdr_types/enum.Pmt.html) to the corresponding message handler:

```bash
curl -X POST -H "Content-Type: application/json" -d '{ "U32": 123 }'  http://127.0.0.1:1337/api/fg/0/block/0/call/freq/
```

Here are some more examples of serialized PMTs:

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

FutureSDR comes with a minimal, work-in-progress web UI, implemented in the *prophecy* crate.
It comes pre-compiled at `crates/prophecy/dist`.
When FutureSDR is started with control port enabled, you can specify the
`frontend_path` [configuration](./running.md#configuration) option to serve a custom
frontend at the root path of the control-port URL (e.g., `127.0.0.1:1337`).

Using the REST API, it is straightforward to build custom UIs.

- A web UI served by an independent server
- A web UI served through FutureSDR control port (see the WLAN and ADS-B examples)
- A UI using arbitrary technology (GTK, Qt, etc.) running as a separate process
  (see the Egui example)


