use anyhow::Result;
use futuresdr::macros::message_handler;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageOutputs;
use futuresdr::runtime::MessageOutputsBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;
use futuresdr::tracing::warn;
use std::collections::VecDeque;

use crate::Encoder;
use crate::Modulator;

pub struct Transmitter {
    frames: VecDeque<Vec<u8>>,
    current_frame: Vec<Complex32>,
    current_offset: usize,
    finished: bool,
    encoder: Encoder,
    modulator: Modulator,
}

impl Transmitter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        code_rate: u8,
        has_crc: bool,
        spreading_factor: u8,
        low_data_rate: bool,
        implicit_header: bool,
        oversampling: usize,
        sync_words: Vec<usize>,
        preamble_len: usize,
        pad: usize,
    ) -> TypedBlock<Self> {
        TypedBlock::new(
            BlockMetaBuilder::new("Transmitter").build(),
            StreamIoBuilder::new()
                .add_output::<Complex32>("out")
                .build(),
            MessageOutputsBuilder::new()
                .add_input("msg", Self::msg_handler)
                .build(),
            Transmitter {
                frames: VecDeque::new(),
                current_frame: Vec::new(),
                current_offset: 0,
                finished: false,
                encoder: Encoder::new(
                    code_rate,
                    spreading_factor,
                    has_crc,
                    low_data_rate,
                    implicit_header,
                ),
                modulator: Modulator::new(
                    spreading_factor.into(),
                    oversampling,
                    sync_words,
                    preamble_len,
                    pad,
                ),
            },
        )
    }

    #[message_handler]
    fn msg_handler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        match p {
            Pmt::Blob(payload) => self.frames.push_back(payload),
            Pmt::String(payload) => self.frames.push_back(payload.as_bytes().into()),
            Pmt::Finished => self.finished = true,
            _ => {
                warn!("Transmitter: Payload was neither String nor Blob");
                return Ok(Pmt::InvalidValue);
            }
        }
        Ok(Pmt::Ok)
    }
}

impl Kernel for Transmitter {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageOutputs<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice::<Complex32>();

        if self.current_offset == self.current_frame.len() {
            if let Some(frame) = self.frames.pop_front() {
                self.current_frame = self.modulator.modulate(self.encoder.encode(frame));
                self.current_offset = 0;
            } else {
                if self.finished {
                    io.finished = true;
                }
                return Ok(());
            }
        }

        let n = std::cmp::min(out.len(), self.current_frame.len() - self.current_offset);
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.current_frame.as_ptr().add(self.current_offset),
                out.as_mut_ptr(),
                n,
            );
        }

        if out.len() > n {
            io.call_again = true;
        }

        self.current_offset += n;
        sio.output(0).produce(n);

        Ok(())
    }
}
