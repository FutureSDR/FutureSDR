#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use async_trait::async_trait;
use futuresdr::anyhow::Result;
use futuresdr::num_complex::Complex;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::mem::size_of;

use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::SoapySourceBuilder;

use futuresdr::runtime::AsyncKernel;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

const SDR_SAMPLERATE: u32 = 1_000_000;

const AUDIO_SAMPLERATE: u32 = 48_000;
const DECIMATION_FACTOR: u32 = 4;
const SDR_RESAMPLERATE: u32 = DECIMATION_FACTOR * AUDIO_SAMPLERATE;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    println!(
        "rrate: {:?}",
        SDR_RESAMPLERATE as f32 / SDR_SAMPLERATE as f32
    );

    let src = SoapySourceBuilder::new()
        .freq(105e6)
        .sample_rate(SDR_SAMPLERATE as f64)
        .gain(20.0)
        .build();
    let snk = AudioSink::new(AUDIO_SAMPLERATE, 1);
    let res = Resampler::new(SDR_RESAMPLERATE as f32 / SDR_SAMPLERATE as f32, None);
    let demod = WBFMDemod::new(SDR_RESAMPLERATE, DECIMATION_FACTOR);

    let src = fg.add_block(src);
    let res = fg.add_block(res);
    let demod = fg.add_block(demod);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", res, "in")?;
    fg.connect_stream(res, "out", demod, "in")?;
    fg.connect_stream(demod, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}

unsafe impl Send for Resampler {}

pub struct Resampler {
    rrate: f32,
    offset: Option<f32>,
    resamp: Option<msresamp_crcf>,
    dc_blocker: Option<iirfilt_crcf>,
    nco: Option<nco_crcf>,
}

impl Resampler {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(rrate: f32, offset: Option<f32>) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("Resampler").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<Complex<f32>>())
                .add_output("out", size_of::<Complex<f32>>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                rrate,
                offset,
                resamp: None,
                dc_blocker: None,
                nco: None,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for Resampler {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex<f32>>();
        let o = sio.output(0).slice::<Complex<f32>>();

        let n = std::cmp::min(i.len(), o.len());

        if n == 0 {
            return Ok(());
        }

        match self.offset {
            Some(offset) => {
                if offset > 0.0 {
                    unsafe {
                        nco_crcf_mix_block_down(
                            self.nco.unwrap(),
                            i.as_mut_ptr() as *mut __BindgenComplex<f32>,
                            i.as_mut_ptr() as *mut __BindgenComplex<f32>,
                            n.try_into().unwrap(),
                        );
                    }
                } else {
                    unsafe {
                        nco_crcf_mix_block_up(
                            self.nco.unwrap(),
                            i.as_mut_ptr() as *mut __BindgenComplex<f32>,
                            i.as_mut_ptr() as *mut __BindgenComplex<f32>,
                            n.try_into().unwrap(),
                        );
                    }
                }
            }
            None => (),
        }
        let mut ny: std::os::raw::c_uint = 0;
        unsafe {
            msresamp_crcf_execute(
                self.resamp.unwrap(),
                i.as_mut_ptr() as *mut __BindgenComplex<f32>,
                n.try_into().unwrap(),
                o.as_mut_ptr() as *mut __BindgenComplex<f32>,
                &mut ny,
            );

            iirfilt_crcf_execute_block(
                self.dc_blocker.unwrap(),
                o.as_mut_ptr() as *mut __BindgenComplex<f32>,
                ny,
                o.as_mut_ptr() as *mut __BindgenComplex<f32>,
            );
        };

        if sio.input(0).finished() && n == i.len() {
            io.finished = true;
        }

        sio.input(0).consume(n);
        sio.output(0).produce(ny.try_into().unwrap());

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.resamp = unsafe { Some(msresamp_crcf_create(self.rrate, 60.0)) };
        self.dc_blocker = unsafe { Some(iirfilt_crcf_create_dc_blocker(0.0005)) };
        self.nco = match self.offset {
            Some(o) => {
                let nco = unsafe { nco_crcf_create(liquid_ncotype_LIQUID_VCO) };
                unsafe { nco_crcf_set_frequency(nco, o) };
                Some(nco)
            }
            None => None,
        };
        Ok(())
    }

    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        unsafe {
            msresamp_crcf_destroy(self.resamp.unwrap());
            if self.nco.is_some() {
                nco_crcf_destroy(self.nco.unwrap());
            }
            iirfilt_crcf_destroy(self.dc_blocker.unwrap());
        };
        Ok(())
    }
}

unsafe impl Send for WBFMDemod {}

pub struct WBFMDemod {
    rate: u32,
    decim: u32,
    fir_decim: Option<firdecim_rrrf>,
    iir_deemph: Option<iirfilt_rrrf>,
    fmdemod: Option<freqdem>,
}

impl WBFMDemod {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(rate: u32, decim: u32) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("WBFMDemod").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<Complex<f32>>())
                .add_output("out", size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                rate,
                decim,
                fir_decim: None,
                iir_deemph: None,
                fmdemod: None,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for WBFMDemod {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex<f32>>();
        let o = sio.output(0).slice::<f32>();

        let mut n = std::cmp::min(i.len(), o.len());

        if n == 0 {
            return Ok(());
        }

        let r = n % usize::try_from(self.decim).unwrap();
        n = n - r;

        let mut tmp = vec![0.0; n];
        unsafe {
            freqdem_demodulate_block(
                self.fmdemod.unwrap(),
                i.as_mut_ptr() as *mut __BindgenComplex<f32>,
                n.try_into().unwrap(),
                tmp.as_mut_ptr(),
            );
            iirfilt_rrrf_execute_block(
                self.iir_deemph.unwrap(),
                tmp.as_mut_ptr(),
                n.try_into().unwrap(),
                tmp.as_mut_ptr(),
            );
            firdecim_rrrf_execute_block(
                self.fir_decim.unwrap(),
                tmp.as_mut_ptr(),
                u32::try_from(n).unwrap() / self.decim,
                o.as_mut_ptr() as *mut f32,
            )
        };

        if sio.input(0).finished() && n == i.len() {
            io.finished = true;
        }

        sio.input(0).consume(n);
        sio.output(0)
            .produce(n / usize::try_from(self.decim).unwrap());

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        self.fir_decim = unsafe { Some(firdecim_rrrf_create_kaiser(self.decim, 10, 60.0)) };
        self.iir_deemph = unsafe {
            Some(iirfilt_rrrf_create_prototype(
                liquid_iirdes_filtertype_LIQUID_IIRDES_BUTTER,
                liquid_iirdes_bandtype_LIQUID_IIRDES_LOWPASS,
                liquid_iirdes_format_LIQUID_IIRDES_SOS,
                2,
                5000.0 / self.rate as f32,
                0.0,
                10.0,
                10.0,
            ))
        };
        self.fmdemod = unsafe { Some(freqdem_create(0.6)) };
        Ok(())
    }

    async fn deinit(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        unsafe {
            freqdem_destroy(self.fmdemod.unwrap());
            iirfilt_rrrf_destroy(self.iir_deemph.unwrap());
            firdecim_rrrf_destroy(self.fir_decim.unwrap());
        };
        Ok(())
    }
}
