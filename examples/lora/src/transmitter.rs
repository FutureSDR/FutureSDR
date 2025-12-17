use anyhow::Result;
use futuresdr::prelude::*;
use std::collections::VecDeque;

use crate::Encoder;
use crate::Modulator;
use crate::utils::CodeRate;
use crate::utils::SpreadingFactor;

#[derive(Block)]
#[message_inputs(msg)]
pub struct Transmitter<O = DefaultCpuWriter<Complex32>>
where
    O: CpuBufferWriter<Item = Complex32>,
{
    #[output]
    output: O,
    frames: VecDeque<Vec<u8>>,
    current_frame: Vec<Complex32>,
    current_offset: usize,
    finished: bool,
    encoder: Encoder,
    modulator: Modulator,
    tag_pending: Option<Tag>,
}

impl<O> Transmitter<O>
where
    O: CpuBufferWriter<Item = Complex32>,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        code_rate: CodeRate,
        has_crc: bool,
        spreading_factor: SpreadingFactor,
        low_data_rate: bool,
        implicit_header: bool,
        oversampling: usize,
        sync_words: Vec<usize>,
        preamble_len: usize,
        pad: usize,
    ) -> Self {
        Self {
            output: O::default(),
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
                spreading_factor,
                oversampling,
                sync_words,
                preamble_len,
                pad,
            ),
            tag_pending: None,
        }
    }

    async fn msg(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
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

impl<O> Kernel for Transmitter<O>
where
    O: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let (out, mut out_tags) = self.output.slice_with_tags();

        if self.current_offset == self.current_frame.len() {
            if let Some(frame) = self.frames.pop_front() {
                self.current_frame = self.modulator.modulate(self.encoder.encode(frame));
                self.current_offset = 0;
                self.tag_pending = Some(Tag::NamedUsize(
                    "burst_start".to_string(),
                    self.current_frame.len(),
                ));
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
        if n > 0 {
            if let Some(tag) = self.tag_pending.take() {
                if let Tag::NamedUsize(_, len) = tag {
                    debug!("Lora TX: tagging burst_start with length {}", len)
                }
                out_tags.add_tag(0, tag);
            }
        } else {
            debug!("produced nothing, out.len() {}", out.len());
        }
        self.current_offset += n;
        self.output.produce(n);

        Ok(())
    }
}
