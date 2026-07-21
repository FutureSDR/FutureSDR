# Bluetooth Low Energy Sniffer

## Introduction

This example implements a Bluetooth Low Energy sniffer for FutureSDR. It
receives BLE 1M signals from an SDR, demodulates GFSK packets, performs
dewhitening and CRC validation, and prints decoded link-layer packet summaries.

The sniffer supports direct reception of a single BLE channel and PFB-based
reception of multiple selected channels. It parses legacy and extended
advertising headers, advertising data, and `CONNECT_IND` parameters. Connection
following uses the captured access address, CRC initialization, channel map,
and channel-selection parameters to inspect BLE data packets on monitored data
channels.

Two decoder modes are available:

- `continuous` processes the complete sample stream and is the default for a
  single channel.
- `burst` detects signal bursts before decoding and is the default for
  multi-channel reception.

## Single-Channel Reception

Run the receiver from this directory:

```sh
cargo run --release
```

The default configuration receives BLE advertising channel 37 with gain 30 and
the continuous decoder. Stop the receiver with Ctrl+C. A fixed duration can be
specified when using SDR hardware:

```sh
cargo run --release -- --channels 37 --gain 30 --duration 30
```

Use the burst decoder explicitly with:

```sh
cargo run --release -- --channels 37 --decoder burst --gain 30
```

Advertising channels 37, 38, and 39 can be selected by channel number:

```sh
cargo run --release -- --channels 38 --decoder burst --gain 30
```

Use `--quiet` to suppress individual packet lines while retaining the final
statistics:

```sh
cargo run --release -- --channels 37 --decoder burst --quiet
```

## Multi-Channel Reception

A comma-separated channel list selects PFB-based multi-channel reception.

```sh
cargo run --release -- --channels 37,0 --gain 30
cargo run --release -- --channels 38,10,11 --gain 30
```

Connection following can be enabled when the channel list contains an
advertising channel and one or more data channels:

```sh
cargo run --release -- --channels 38,10,11 --follow-connections --gain 30
```

The follower records CRC-valid `CONNECT_IND` packets, predicts CSA #1 and
CSA #2 connection events, validates data-channel packets with the
connection-specific access address and CRC initialization, and processes
link-layer connection and channel-map updates.

The continuous decoder can also be selected explicitly for a multi-channel
flowgraph:

```sh
cargo run --release -- --channels 38,10,11 --decoder continuous --gain 30
```

## Simulation

Simulation mode generates BLE advertising packets and passes them through the
GMSK receive chain without SDR hardware:

```sh
cargo run --release -- --simulate --decoder burst --simulate-packets 10
```

The flowgraph runs until all generated samples have been processed.

## Offline IQ Input

Complex `f32` IQ recordings can be decoded with `--iq-file`. Samples use the
native `Complex32` layout expected by FutureSDR's `FileSource`, with interleaved
little-endian values:

```text
I0 Q0 I1 Q1 I2 Q2 ...
```

The selected channel must match the recording center frequency, and the sample
rate must match the recorded waveform:

```sh
cargo run --release -- --iq-file ble_capture.iq --sample-rate 2e6 --channels 37 --decoder burst --quiet
```

File input runs to EOF and then shuts down automatically.

## Wireshark

Decoded advertising and data packets can be streamed live to Wireshark over
UDP using RFTap encapsulation:

```sh
cargo run --release -- --channels 37 --decoder burst --wireshark
```

The default destination is `127.0.0.1:55556`. Capture the loopback interface in
Wireshark and configure UDP port 55556 with **Decode As...** `RFTap`. The `btle`
display filter then shows decoded BLE link-layer packets.

Useful display filters include:

```text
btle
btle.access_address == 0x8e89bed6
btle && btle.access_address != 0x8e89bed6
```

The destination can be changed when required:

```sh
cargo run --release -- --channels 37 --decoder burst --wireshark --wireshark-addr 127.0.0.1:55556
```

## Output

Decoded packets are printed in a compact form:

```text
BLE packet: ch=37 phy=1M pdu=ADV_IND payload_len=24 adv_a=57:41:78:10:B4:06(random) ad=[flags=0x1A, tx_power=12dBm, mfg=Apple(0x004C)/8B]
BLE packet: ch=38 phy=1M pdu=ADV_EXT_IND payload_len=7 ext=[mode=connectable adi=0x3F02 aux_ch=31 aux_offset=1170us aux_phy=2M]
```

On shutdown, each decoder prints CRC and packet statistics:

```text
BLE summary: ch=37 mode=continuous packets=8738 connect_ind=0 crc_passes=8738 crc_checks=12613 crc_pass_rate=69.3%
BLE summary: ch=37 mode=burst packets=9397 connect_ind=0 crc_passes=8908 crc_checks=10378 crc_pass_rate=85.8%
```

When connection following is enabled, the receiver also reports tracked data
packets, connection state, timing recovery, and SDR overflow events.
