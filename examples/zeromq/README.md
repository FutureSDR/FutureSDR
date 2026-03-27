# ZeroMQ Example

## Introduction

This example demonstrates how to stream data between two FutureSDR applications using ZeroMQ. It highlights the framework's ability to handle network-based data distribution across different processes.

## How It Works

The example consists of a sender flowgraph and a receiver flowgraph.

* zmq-sender:
    - NullSource: Generates a stream of null bytes.
    - Head: Limits the stream to 1,000,000 samples.
    - Throttle: Regulates the flow to 100 kHz.
    - PubSink: Publishes the data over TCP port 50001.

* zmq-receiver:
    - SubSource: Connects to the sender's address and subscribes to the stream.
    - FileSink: Records the incoming data into a local binary file (`zmq-log.bin`).

## How to Run

To run the inter-process communication example, use two separate terminals.

Start the receiver first to prepare the data sink. To avoid conflicts, change the default control port:

```sh
FUTURESDR_CTRLPORT_BIND=127.0.0.1:1338 cargo run --release --bin zmq-receiver
```

In a second terminal, start the publisher:

```sh
cargo run --release --bin zmq-sender
```

Once the sender finishes sending its 1 million samples, the transfer will complete and the `zmq-log.bin` file will be written to your project directory.
