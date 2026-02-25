SDR Overflow & Timeout Modeling Example (overflow_test)
=======================

## Introduction
This example is designed to model and observe how Windows systems handle data congestion (Overflow) and communication loss (Timeout) in Software Defined Radio (SDR) applications. It is particularly useful for debugging applications that integrate SDR sources with UI, Web interfaces, or sound cards.

## How It Works
- The flowgraph consists of a **Seify Source (USRP/HackRF)**, a **Throttle** block, and a **Null Sink**. 
- By default, the `throttle_rate` is set higher than the `source_sample_rate`, allowing the data to flow freely. 

- The core of this model is to dynamically change the `throttle_rate` while the flowgraph is running. By using the FutureSDR REST API, we can simulate a sudden CPU bottleneck or a slow processing block, causing the hardware buffers to overflow.

## How to Run

1. Run the example:

```sh
cargo run --release
  ```

2. While the application is running, use a Python script to drop the throttle rate below the source sample rate. This simulates a block that cannot keep up with the incoming data:

 ```python
  import requests

  asdf = requests.post("http://localhost:1337/api/fg/0/block/2/call/rate/", json={"F64": 1000000}) 
  ```

3. Observe device behaviors:
    * USRP: Typically, an Overflow warning is immediately followed by a Timeout Error, which causes the flowgraph to stop.
    * HackRF: Usually continues to run while spamming continuous Overflow warnings.

It shows that any delay in the software -like our **Throttle** block- immediately affects the hardware. This causes the device to lose sync, leading to either a total stream crash (as seen in USRP) or continuous data corruption (as seen in HackRF). This explains the performance drops often seen in Windows-based SDR applications involving audio or GUI components.