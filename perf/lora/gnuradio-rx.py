#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import os
import signal
import sys
import time
from argparse import ArgumentParser
from contextlib import contextmanager

import numpy as np
from gnuradio import blocks
from gnuradio import gr
from gnuradio import lora_sdr

# Parameters matching the compatible FutureSDR file receiver.
BRANCHES = 4
SF = 7
SAMPLE_RATE = 1_000_000
BANDWIDTH = 125_000
OVERSAMPLE = SAMPLE_RATE // BANDWIDTH
CENTER_FREQ = 869_525_000
SYNC_WORD = [0x34]  # public LoRa sync word
SOFT_DECODING = False
LDRO_MODE = False
PREAMBLE_LEN = 8
PRINT_HEADER = False
PRINT_RX_MSG = False

DEFAULT_FILE = "lora-dumps/lora_single_channel_1M_multi_sf_16B_1dBpSF_300s.cf32"


def load_cf32(file):
    dtype = np.dtype("<c8")
    size = os.path.getsize(file)
    if size % dtype.itemsize != 0:
        raise ValueError(
            f"invalid cf32 file size ({size}), expected multiple of {dtype.itemsize} bytes"
        )
    return np.fromfile(file, dtype=dtype)


def fileno(file_or_fd):
    fd = getattr(file_or_fd, "fileno", lambda: file_or_fd)()
    if not isinstance(fd, int):
        raise ValueError("Expected a file (`.fileno()`) or a file descriptor")
    return fd


@contextmanager
def stdout_redirected(to=os.devnull, stdout=None):
    if stdout is None:
        stdout = sys.stdout

    stdout_fd = fileno(stdout)
    with os.fdopen(os.dup(stdout_fd), "wb") as copied:
        stdout.flush()
        with open(to, "wb") as to_file:
            os.dup2(to_file.fileno(), stdout_fd)
        try:
            yield stdout
        finally:
            stdout.flush()
            os.dup2(copied.fileno(), stdout_fd)


class lora_RX(gr.top_block):
    def __init__(self, file=DEFAULT_FILE):
        gr.top_block.__init__(self, "Lora Rx", catch_exceptions=True)

        self.file = file
        self.soft_decoding = SOFT_DECODING
        self.pay_len = 11
        self.impl_head = False
        self.has_crc = True
        self.cr = 1

        samples = load_cf32(file)
        self.sources = []
        self.message_stores = []

        for _ in range(BRANCHES):
            source = blocks.vector_source_c(samples, False, 1, [])
            self.sources.append(source)
            header_decoder = lora_sdr.header_decoder(
                self.impl_head, self.cr, self.pay_len, self.has_crc, LDRO_MODE, PRINT_HEADER
            )
            hamming_dec = lora_sdr.hamming_dec(self.soft_decoding)
            gray_mapping = lora_sdr.gray_mapping(self.soft_decoding)
            frame_sync = lora_sdr.frame_sync(
                int(CENTER_FREQ),
                BANDWIDTH,
                SF,
                self.impl_head,
                SYNC_WORD,
                OVERSAMPLE,
                PREAMBLE_LEN,
            )
            fft_demod = lora_sdr.fft_demod(self.soft_decoding, True)
            dewhitening = lora_sdr.dewhitening()
            deinterleaver = lora_sdr.deinterleaver(self.soft_decoding)
            crc_verif = lora_sdr.crc_verif(PRINT_RX_MSG, False)
            crc_ok_sink = blocks.null_sink(gr.sizeof_char)
            crc_bad_sink = blocks.null_sink(gr.sizeof_char)
            message_debug = blocks.message_debug()
            self.message_stores.append(message_debug)

            self.msg_connect((header_decoder, "frame_info"), (frame_sync, "frame_info"))
            self.msg_connect((crc_verif, "msg"), (message_debug, "store"))
            self.connect((source, 0), (frame_sync, 0))
            self.connect((frame_sync, 0), (fft_demod, 0))
            self.connect((fft_demod, 0), (gray_mapping, 0))
            self.connect((gray_mapping, 0), (deinterleaver, 0))
            self.connect((deinterleaver, 0), (hamming_dec, 0))
            self.connect((hamming_dec, 0), (header_decoder, 0))
            self.connect((header_decoder, 0), (dewhitening, 0))
            self.connect((dewhitening, 0), (crc_verif, 0))
            self.connect((crc_verif, 0), (crc_ok_sink, 0))
            self.connect((crc_verif, 1), (crc_bad_sink, 0))


def count_messages(message_debug):
    count = 0
    while True:
        try:
            message_debug.get_message(count)
            count += 1
        except Exception:
            return count


def argument_parser():
    parser = ArgumentParser()
    parser.add_argument(
        "--run", dest="run", type=int, default=0, help="Set run number [default=%(default)r]"
    )
    parser.add_argument(
        "-f",
        "--file",
        dest="file",
        type=str,
        default=DEFAULT_FILE,
        help="Set file name [default=%(default)r]",
    )
    return parser


def main(top_block_cls=lora_RX, options=None):
    if options is None:
        options = argument_parser().parse_args()

    with stdout_redirected():
        tb = top_block_cls(file=options.file)

        def sig_handler(sig=None, frame=None):
            tb.stop()
            tb.wait()
            sys.exit(0)

        signal.signal(signal.SIGINT, sig_handler)
        signal.signal(signal.SIGTERM, sig_handler)

        start_time = time.time()
        tb.start()
        tb.wait()
        elapsed = time.time() - start_time

    counts = [count_messages(message_store) for message_store in tb.message_stores]
    print(
        "{},{},legacy,{},{},{},{},{}".format(
            options.run, options.file, elapsed, counts[0], counts[1], counts[2], counts[3]
        )
    )


if __name__ == "__main__":
    main()
