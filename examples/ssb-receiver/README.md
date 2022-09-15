SSB Decoding
============

Goals is to achieve SSB decoding as in:
* https://wiki.gnuradio.org/index.php/Simulation_example:_Single_Sideband_transceiver
* http://www.csun.edu/~skatz/katzpage/sdr_project/sdr/grc_tutorial4.pdf

So really have same result as this [GNURadio flowgraph](./ssb-decoder.grc).

## Prerequisite

Download file from https://www.csun.edu/~skatz/katzpage/sdr_project/sdr/ssb_lsb_256k_complex2.dat.zip

## Troubleshooting

- [x] Proper frequency translating
- [x] Use low-pass filter
- ~~[ ] Replace downsampling by decimation (ie with lowpass filtering)~~ Not needed because what is done with 2 previous points is working in a similar way than GNURadio.

![](flowgraph-2022-07-28-124646.png)
