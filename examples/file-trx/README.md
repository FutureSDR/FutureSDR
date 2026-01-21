File Transmit and Receive Example (file-trx)
========================

## Introduction

This directory contains examples for transmitting and receiving radio signals using files as sources or destinations.


## Tx

To broadcast a previously recorded file:
```sh
cargo run --release --bin tx --input {file_name}.{file_format} --frequency {center_frequency} --gain {gain} --repeat
```
Input file formats can be either `cs8` or `cf32`.

## Rx
- To read a previously recorded `cs8` file and output it as a `cf32` file:
```sh
cargo run --release --bin rx -- --input {file_name}.{file_format} --out {file_name}.{file_format}
```

- To record a signal from a specific frequency to a file:
```sh
cargo run --release --bin rx -- --out {file_name}.{file_format} --frequency {center_frequency} --rate {sampling_rate} --samples {num_of_received_samples}
```
* Output file formats can be either `cs8` or `cf32`.
* The average and maximum signal magnitudes can be monitored in the terminal.



