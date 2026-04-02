#!/usr/bin/env python3
# -*- coding: utf-8 -*-

#
# SPDX-License-Identifier: GPL-3.0
#
# GNU Radio Python Flow Graph
# Title: Wifi Tx
# GNU Radio version: 3.10.12.0

import os
import sys
import logging as log

def get_state_directory() -> str:
    oldpath = os.path.expanduser("~/.grc_gnuradio")
    try:
        from gnuradio.gr import paths
        newpath = paths.persistent()
        if os.path.exists(newpath):
            return newpath
        if os.path.exists(oldpath):
            log.warning(f"Found persistent state path '{newpath}', but file does not exist. " +
                     f"Old default persistent state path '{oldpath}' exists; using that. " +
                     "Please consider moving state to new location.")
            return oldpath
        # Default to the correct path if both are configured.
        # neither old, nor new path exist: create new path, return that
        os.makedirs(newpath, exist_ok=True)
        return newpath
    except (ImportError, NameError):
        log.warning("Could not retrieve GNU Radio persistent state directory from GNU Radio. " +
                 "Trying defaults.")
        xdgstate = os.getenv("XDG_STATE_HOME", os.path.expanduser("~/.local/state"))
        xdgcand = os.path.join(xdgstate, "gnuradio")
        if os.path.exists(xdgcand):
            return xdgcand
        if os.path.exists(oldpath):
            log.warning(f"Using legacy state path '{oldpath}'. Please consider moving state " +
                     f"files to '{xdgcand}'.")
            return oldpath
        # neither old, nor new path exist: create new path, return that
        os.makedirs(xdgcand, exist_ok=True)
        return xdgcand

sys.path.append(os.environ.get('GRC_HIER_PATH', get_state_directory()))

from gnuradio import analog
from gnuradio import blocks
import numpy
from gnuradio import gr
from gnuradio.filter import firdes
from gnuradio.fft import window
import signal
from argparse import ArgumentParser
from gnuradio.eng_arg import eng_float, intx
from gnuradio import eng_notation
from gnuradio import gr, pdu
from wifi_phy_hier import wifi_phy_hier  # grc-generated hier_block
import foo
import ieee802_11
import threading




class wifi_tx(gr.top_block):

    def __init__(self):
        gr.top_block.__init__(self, "Wifi Tx", catch_exceptions=True)
        self.flowgraph_started = threading.Event()

        ##################################################
        # Variables
        ##################################################
        self.out_buf_size = out_buf_size = 96000
        self.l = l = 1000

        ##################################################
        # Blocks
        ##################################################

        self.wifi_phy_hier_0 = wifi_phy_hier(
            bandwidth=20e6,
            chan_est=ieee802_11.LS,
            encoding=ieee802_11.Encoding(4),
            frequency=2.4e9,
            sensitivity=0.56,
        )
        self.pdu_tagged_stream_to_pdu_0 = pdu.tagged_stream_to_pdu(gr.types.byte_t, 'packet_len')
        self.ieee802_11_mac_0 = ieee802_11.mac([0x23, 0x23, 0x23, 0x23, 0x23, 0x23], [0x42, 0x42, 0x42, 0x42, 0x42, 0x42], [0xff, 0xff, 0xff, 0xff, 0xff, 255])
        self.foo_packet_pad2_0 = foo.packet_pad2(False, False, 0.01, 5000, 5000)
        self.foo_packet_pad2_0.set_min_output_buffer(out_buf_size)
        self.blocks_vector_source_x_0 = blocks.vector_source_c((0,), False, 1, [])
        self.blocks_throttle2_0 = blocks.throttle( gr.sizeof_char*1, 100000, True, 0 if "auto" == "auto" else max( int(float(0.1) * 100000) if "auto" == "time" else int(0.1), 1) )
        self.blocks_stream_to_tagged_stream_0 = blocks.stream_to_tagged_stream(gr.sizeof_char, 1, l, "packet_len")
        self.blocks_multiply_const_vxx_0 = blocks.multiply_const_cc(0.6)
        self.blocks_multiply_const_vxx_0.set_min_output_buffer(100000)
        self.blocks_head_0 = blocks.head(gr.sizeof_char*1, (10000 * l))
        self.blocks_file_sink_0 = blocks.file_sink(gr.sizeof_gr_complex*1, f"/home/basti/src/futuresdr/perf/wlan/wlan-{l}.cf32", False)
        self.blocks_file_sink_0.set_unbuffered(False)
        self.blocks_add_xx_0 = blocks.add_vcc(1)
        self.analog_random_source_x_0 = blocks.vector_source_b(list(map(int, numpy.random.randint(0, 255, 1000))), True)
        self.analog_fastnoise_source_x_0 = analog.fastnoise_source_c(analog.GR_GAUSSIAN, 0.01, 0, 8192)


        ##################################################
        # Connections
        ##################################################
        self.msg_connect((self.ieee802_11_mac_0, 'phy out'), (self.wifi_phy_hier_0, 'mac_in'))
        self.msg_connect((self.pdu_tagged_stream_to_pdu_0, 'pdus'), (self.ieee802_11_mac_0, 'app in'))
        self.connect((self.analog_fastnoise_source_x_0, 0), (self.blocks_add_xx_0, 0))
        self.connect((self.analog_random_source_x_0, 0), (self.blocks_throttle2_0, 0))
        self.connect((self.blocks_add_xx_0, 0), (self.blocks_file_sink_0, 0))
        self.connect((self.blocks_head_0, 0), (self.blocks_stream_to_tagged_stream_0, 0))
        self.connect((self.blocks_multiply_const_vxx_0, 0), (self.foo_packet_pad2_0, 0))
        self.connect((self.blocks_stream_to_tagged_stream_0, 0), (self.pdu_tagged_stream_to_pdu_0, 0))
        self.connect((self.blocks_throttle2_0, 0), (self.blocks_head_0, 0))
        self.connect((self.blocks_vector_source_x_0, 0), (self.wifi_phy_hier_0, 0))
        self.connect((self.foo_packet_pad2_0, 0), (self.blocks_add_xx_0, 1))
        self.connect((self.wifi_phy_hier_0, 0), (self.blocks_multiply_const_vxx_0, 0))


    def get_out_buf_size(self):
        return self.out_buf_size

    def set_out_buf_size(self, out_buf_size):
        self.out_buf_size = out_buf_size

    def get_l(self):
        return self.l

    def set_l(self, l):
        self.l = l
        self.blocks_head_0.set_length((10000 * self.l))
        self.blocks_stream_to_tagged_stream_0.set_packet_len(self.l)
        self.blocks_stream_to_tagged_stream_0.set_packet_len_pmt(self.l)




def main(top_block_cls=wifi_tx, options=None):
    tb = top_block_cls()

    def sig_handler(sig=None, frame=None):
        tb.stop()
        tb.wait()

        sys.exit(0)

    signal.signal(signal.SIGINT, sig_handler)
    signal.signal(signal.SIGTERM, sig_handler)

    tb.start()
    tb.flowgraph_started.set()

    tb.wait()


if __name__ == '__main__':
    main()
