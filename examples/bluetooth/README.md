# Bluetooth Sniffer

## Introduction

This example is an experimental BLE sniffer for FutureSDR. It currently focuses
on BLE advertising reception and provides a working baseline for later
connection following and wider multi-channel reception.

The receiver can tune one BLE channel directly, or use a FutureSDR PFB
channelizer to split a wider capture into multiple 2 MHz BLE channel bins. The
preferred decoder path is the IQ burst decoder. A continuous decoder is still
available as a reference/debug path.

The example can currently:

- receive BLE advertising channels with SDR hardware,
- demodulate BLE 1M GFSK packets,
- detect the advertising access address,
- dewhiten and CRC-check advertising PDUs,
- parse common legacy advertising packet types,
- summarize advertising data fields,
- parse common `ADV_EXT_IND` extended advertising header fields,
- parse `CONNECT_IND` and store connection parameters in a small connection
  table,
- run a PFB multi-channel mode for selected BLE channels,
- validate the PFB channel mapping with a synthetic unit test.

This is not yet a full Bluetooth sniffer. It does not yet decode BLE data
channel packets or follow connection hopping.

## Running

Run commands from this directory:

```sh
cargo run --release
```

The default channel is BLE advertising channel 37 and the default gain is 30.
Stop the sniffer with Ctrl+C. The flowgraph shuts down gracefully and prints
final packet statistics.

For a fixed-duration run:

```sh
cargo run --release -- --duration 30
```

The burst decoder is the recommended path:

```sh
cargo run --release -- --channel 37 --burst --gain 30 --duration 30
```

Packet printing can be disabled while keeping final statistics:

```sh
cargo run --release -- --channel 37 --burst --quiet --gain 30 --duration 30
```

To tune another advertising channel:

```sh
cargo run --release -- --channel 38 --burst --quiet --gain 30 --duration 30
cargo run --release -- --channel 39 --burst --quiet --gain 30 --duration 30
```

For a USRP B210, the following arguments were useful in local tests:

```sh
cargo run --release -- --channel 37 --burst --quiet --gain 30 --duration 30 --args "type=b200,num_recv_frames=512"
```

## Simulation

Simulation mode generates synthetic BLE advertising packets and sends them
through the GMSK receive chain:

```sh
cargo run --release -- --simulate --burst --simulate-packets 3
```

This is useful for checking the parser, whitening, CRC, GMSK filter, burst
detector, demodulator, and packet detector without SDR hardware.

## Multi-Channel Mode

Multi-channel mode uses FutureSDR's `PfbChannelizer` to split a wider capture
into 2 MHz channel bins. The channel list selects which BLE channels are decoded.
Passive channels are included in the PFB capture span but are connected to a
sink instead of a BLE decoder. This is useful for measuring whether a wider span
is practical without implementing data-channel decoding yet.

Advertising channel 37 plus adjacent data channel 0:

```sh
cargo run --release -- --multi-channel --channels 37 --passive-channels 0 --burst --quiet --gain 30 --duration 30 --args "type=b200,num_recv_frames=512"
```

Advertising channel 38 plus nearby data channels:

```sh
cargo run --release -- --multi-channel --channels 38 --passive-channels 10,11 --burst --quiet --gain 30 --duration 30 --args "type=b200,num_recv_frames=512"
cargo run --release -- --multi-channel --channels 38 --passive-channels 9,10,11,12 --burst --quiet --gain 30 --duration 30 --args "type=b200,num_recv_frames=512"
```

Advertising channels 37 and 38 together:

```sh
cargo run --release -- --multi-channel --channels 37,38 --burst --quiet --gain 30 --duration 30 --args "type=b200,num_recv_frames=512"
```

In local B210 tests, low-rate PFB captures around one advertising channel were
stable. Capturing advertising channels 37 and 38 together required a much wider
span and produced many overflows on the test laptop, although packets from both
channels were still decoded.

## Output

Example packet summaries:

```text
BLE packet: ch=37 type=ADV_IND len=24 adv_a=57:41:78:10:B4:06 ad=[flags=0x1A, tx_power=12dBm, mfg=Apple(0x004C)/8B]
BLE packet: ch=37 type=ADV_EXT_IND len=7 ext=[mode=connectable adi=0x3F02 aux_ch=31 aux_offset=1170us aux_phy=2M]
```

At shutdown the example prints final statistics and any tracked connections:

```text
BLE burst stats: ch=37 bursts=15757 decoded_bursts=11345 short_bursts=0 fsk_rejects=1881 packets=11345 connect_ind=1 connections=1 aa_candidates=13647 aa_per_packet=1.20 header_rejects=2 pdu_attempts=12833 crc_rejects=1488 crc_reject_rate=11.6%
BLE connection: ch=37 aa=0xC6176D8D crc_init=0xA9DABD init_a=6D:3B:2F:E7:19:8A(random) adv_a=78:4B:F4:DF:75:14(random) interval=30.00ms latency=0 timeout=5000ms hop=14 sca=5 channels=37 first_ch=37 last_ch=37 seen=1
```

Statistics:

- `packets`: CRC-valid BLE advertising packets.
- `connect_ind`: CRC-valid `CONNECT_IND` packets.
- `connections`: unique connection requests stored in the connection table.
- `aa_candidates`: detected advertising access address candidates.
- `aa_per_packet`: access-address candidates per valid packet.
- `header_rejects`: candidates rejected before CRC because no plausible PDU
  length was found.
- `pdu_attempts`: valid packets plus non-duplicate CRC rejects.
- `crc_rejects`: candidate windows that reached CRC and failed.
- `crc_reject_rate`: `crc_rejects / pdu_attempts`.

The continuous decoder additionally reports:

- `duplicate_crc_rejects`: nearby CRC rejects grouped into the same candidate
  window.
- `raw_crc_rejects`: `crc_rejects + duplicate_crc_rejects`.

## Current Measurements

These measurements were taken with a USRP B210, gain 30, packet printing
disabled, and `num_recv_frames=512`. They are intended as rough development
guidance, not final benchmark numbers.

Single-channel burst decoder:

| Channel | Packets | CONNECT_IND | CRC Reject Rate |
| ------- | ------- | ----------- | --------------- |
| 37 | 5427 | 1 | 11.7% |
| 38 | 4811 | 1 | 11.5% |
| 39 | 4000 | 1 | 13.1% |

Low-rate PFB captures:

| Configuration | Packets | CONNECT_IND | CRC Reject Rate |
| ------------- | ------- | ----------- | --------------- |
| ch37 + passive data0 | 5776 | 0 | 11.8% |
| ch38 + passive data10,11 | 5398 | 1 | 13.0% |
| ch38 + passive data9,10,11,12 | 6152 | 0 | 14.8% |

Two advertising channels through PFB:

| Channel | Packets | CONNECT_IND | CRC Reject Rate |
| ------- | ------- | ----------- | --------------- |
| 37 | 1088 | 0 | 14.8% |
| 38 | 992 | 0 | 12.9% |

The two-advertising-channel run produced many USRP overflows on the test
machine. This suggests that the current PFB path is functionally correct but
wide captures are limited by host/USB/PFB throughput in this setup.

Continuous decoder versus burst decoder on channel 37:

| Decoder | Packets | CRC Reject Rate | AA/Packet | Header Rejects | Avg CPU | Max CPU |
| ------- | ------- | --------------- | --------- | -------------- | ------- | ------- |
| Continuous | 16154 | 26.5% | 2.26 | 146 | 7.6% | 10.2% |
| Burst | 12632 | 13.1% | 1.21 | 4 | 3.8% | 5.7% |

The burst decoder used roughly half the CPU and produced fewer rejects in this
measurement. The continuous decoder is currently kept as a reference/debug path.

## Decoder Notes

The burst decoder:

1. Applies the GMSK receive filter.
2. Detects IQ bursts with a simple squelch.
3. Estimates and removes burst-level frequency offset.
4. Demodulates and slices the burst.
5. Searches for the BLE advertising access address.
6. Tries nearby bit offsets and accepts the first CRC-valid advertising PDU.
7. Records `CONNECT_IND` packets in a connection table.

The continuous decoder:

1. Applies the GMSK receive filter.
2. Runs a phase discriminator.
3. Slices discriminator output into bits with DC tracking.
4. Searches for the BLE advertising access address on each sample phase.
5. Captures a short PDU bit window after an access address candidate.
6. Tries nearby bit offsets and accepts the first CRC-valid advertising PDU.
7. Groups nearby CRC failures into candidate-window rejects.

The `--normalize-levels` flag enables an experimental slicer mode for the
continuous decoder. It is disabled by default because it did not provide a
consistent improvement in local measurements.

## Current Scope and Limitations

- BLE 1M advertising reception is the current focus.
- Burst decoding is the preferred path.
- `CONNECT_IND` is parsed and stored, but BLE data channel decoding is not
  implemented yet.
- Channel hopping is not followed yet.
- `ADV_EXT_IND` common header fields are parsed, but secondary advertising
  channels from `AuxPtr` are not followed yet.
- Multi-channel PFB mode works for selected spans, but wide captures can be
  limited by host throughput.
- No PCAP, RFTap, Wireshark, or UDP output yet.

## Next Steps

- Use the connection table to try data-channel access-address detection.
- CRC-check data-channel packets with the captured `crc_init`.
- Add channel-map and hop prediction for one tracked connection.
- Decide whether the continuous decoder should remain as a debug/reference path
  or be removed.
- Investigate whether the current FutureSDR PFB configuration is appropriate
  for wider BLE captures, or whether a lighter/optimized channelizer path is
  needed.
- Add PCAP/RFTap output for Wireshark inspection.
