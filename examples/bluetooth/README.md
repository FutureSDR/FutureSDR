# Bluetooth Sniffer

## Introduction

This example is a single-channel BLE advertising sniffer. It receives a BLE
advertising channel with an SDR, demodulates the GFSK signal, detects the BLE
advertising access address, dewhitens the advertising PDU, validates the BLE
CRC, and prints a compact packet summary.

The current implementation is intended as a baseline for further work. It is
not yet a full Bluetooth sniffer and does not follow connections or decode all
Bluetooth PHY modes.

## Running

The default channel is BLE advertising channel 37. The default gain is 30,
which provided the best balance between packet count and CRC reject rate in
local tests.

From the repository root:

```sh
cargo run --release
```

Stop the sniffer with Ctrl+C. The flowgraph shuts down gracefully and prints
final packet statistics.

For a fixed-duration run:

```sh
cargo run --release -- --duration 30
```

To tune another advertising channel:

```sh
cargo run --release -- --channel 38
cargo run --release -- --channel 39
```

## Simulation

Simulation mode generates synthetic BLE advertising packets and sends them
through the GMSK receive chain:

```sh
cargo run --release -- --simulate --simulate-packets 3
```

This is useful for checking the parser, whitening, CRC, GMSK filter,
discriminator, slicer, and packet detector without SDR hardware.

## Output

Example output:

```text
BLE packet: ch=37 type=ADV_IND len=24 adv_a=57:41:78:10:B4:06 ad=[flags=0x1A, tx_power=12dBm, mfg=Apple(0x004C)/8B]
BLE packet: ch=37 type=ADV_EXT_IND len=7 ext=[mode=connectable adi=0x3F02 aux_ch=31 aux_offset=1170us aux_phy=2M]
```

At shutdown the example prints final statistics:

```text
BLE stats: packets=1695 aa_candidates=3630 aa_per_packet=2.14 header_rejects=5 pdu_attempts=2210 crc_rejects=515 duplicate_crc_rejects=300 raw_crc_rejects=815 crc_reject_rate=23.3%
```

Statistics:

- `packets`: CRC-valid BLE advertising packets.
- `aa_candidates`: detected advertising access address candidates.
- `aa_per_packet`: access-address candidates per valid packet.
- `header_rejects`: candidates rejected before CRC because no plausible PDU length was found.
- `pdu_attempts`: valid packets plus non-duplicate CRC rejects.
- `crc_rejects`: candidate windows that reached CRC and failed.
- `duplicate_crc_rejects`: nearby CRC rejects grouped into the same candidate window.
- `raw_crc_rejects`: `crc_rejects + duplicate_crc_rejects`.
- `crc_reject_rate`: `crc_rejects / pdu_attempts`.

## Decoder Notes

The receiver currently uses a continuous detector:

1. Filter complex baseband samples with a GMSK receive filter.
2. Run a phase discriminator.
3. Slice discriminator output into bits with DC tracking.
4. Search for the BLE advertising access address on each sample phase.
5. After an access address candidate, capture a short PDU bit window.
6. Try nearby bit offsets and accept the first CRC-valid PDU.
7. Group nearby CRC failures into candidate-window rejects.

The `--normalize-levels` flag enables an experimental slicer mode that tracks
positive and negative discriminator levels. It is disabled by default because it
did not provide a consistent improvement in local measurements.

## Current Scope and Limitations

- Single tuned BLE channel only.
- Default channel is advertising channel 37.
- Legacy advertising PDUs are parsed and summarized.
- `ADV_EXT_IND` common extended header fields are summarized when present.
- Secondary advertising channels from `AuxPtr` are not followed yet.
- BLE data channels and connection following are not implemented.
- No PCAP, RFTap, Wireshark, or UDP output yet.
- No PFB channelizer yet.

## Next Steps

- Add README-level examples and measurement guidance for gain selection.
- Add PCAP/RFTap output for Wireshark inspection.
- Improve burst-based demodulation and timing/CFO handling.
- Add channel hopping or a multi-channel PFB receiver.
- Use `ADV_EXT_IND` `AuxPtr` information to follow secondary advertising once
  multi-channel reception is available.