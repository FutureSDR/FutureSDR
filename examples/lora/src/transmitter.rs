use anyhow::Result;
use futuresdr::runtime::dev::prelude::*;
use std::collections::VecDeque;

use crate::Encoder;
use crate::HeaderMode;
use crate::Modulator;
use crate::utils::CodeRate;
use crate::utils::SpreadingFactor;
use crate::utils::SynchWord;

#[derive(Block)]
#[message_inputs(msg, synch_word)]
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
        ldro_enabled: bool,
        header_mode: HeaderMode,
        oversampling: usize,
        sync_words: SynchWord,
        preamble_len: usize,
        pad: usize,
    ) -> Result<Self> {
        Ok(Self {
            output: O::default(),
            frames: VecDeque::new(),
            current_frame: Vec::new(),
            current_offset: 0,
            finished: false,
            encoder: Encoder::new(
                code_rate,
                spreading_factor,
                has_crc,
                ldro_enabled,
                !matches!(header_mode, HeaderMode::Explicit),
            ),
            modulator: Modulator::new(
                spreading_factor,
                oversampling,
                sync_words,
                preamble_len,
                pad,
            )?,
            tag_pending: None,
        })
    }

    async fn msg(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
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

    async fn synch_word(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        let new_synch_word = match p {
            Pmt::U32(synch_word_new) => SynchWord::from(TryInto::<u8>::try_into(synch_word_new)?),
            Pmt::U64(synch_word_new) => SynchWord::from(TryInto::<u8>::try_into(synch_word_new)?),
            Pmt::Blob(synch_word_new) => TryFrom::<&[u8]>::try_from(&synch_word_new)?,
            Pmt::Finished => {
                self.finished = true;
                return Ok(Pmt::Ok);
            }
            _ => {
                warn!("Transmitter: new synch_word was not a Blob");
                return Ok(Pmt::InvalidValue);
            }
        };
        self.modulator.set_synch_word(new_synch_word)?;
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
        _mo: &mut MessageOutputs,
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
