# File Transmit and Receive Example

## Introduction

This directory contains examples for receiving radio signals to files and transmitting radio signals from files.

## TX

To send a previously recorded file, plug in your SDR and run the following command:
```sh
cargo run --release --bin tx -- --input {file_name}.{file_format} --frequency {center_frequency} --gain {gain} --repeat
```

Input file formats can be either `cs8` or `cf32`.

## RX

To record a signal at a specific frequency to a file, plug in your SDR and run the following command:

```sh
cargo run --release --bin rx -- --out {file_name}.{file_format} --frequency {center_frequency} --rate {sampling_rate} --samples {num_of_received_samples}
```

Output file formats can be either `cs8` or `cf32`. The average and maximum signal magnitudes can be monitored in the terminal.

## Convert File Formats

To read a previously recorded `cs8` or `cf32` file and write it as a `cf32` or `cs8` file, run:

```sh
cargo run --release --bin rx -- --input {file_name}.{file_format} --out {file_name}.{file_format}
```
