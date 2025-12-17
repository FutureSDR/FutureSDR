use futuresdr::prelude::*;
use std::collections::HashMap;

use crate::utils::*;

#[derive(Block)]
pub struct Deinterleaver<
    S = DemodulatedSymbolSoftDecoding,
    D = DeinterleavedSymbolSoftDecoding,
    I = DefaultCpuReader<DemodulatedSymbolSoftDecoding>,
    O = DefaultCpuWriter<DeinterleavedSymbolSoftDecoding>,
> where
    S: DemodulatedSymbol,
    D: DeinterleavedSymbol,
    I: CpuBufferReader<Item = S>,
    O: CpuBufferWriter<Item = D>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    sf: usize,       // Spreading factor
    cr: usize,       // Coding rate
    is_header: bool, // Indicate that we need to deinterleave the first block with the default header parameters (cr=4/8, reduced rate)
    ldro: bool,      // use low datarate optimization mode
}

impl<S, D, I, O> Deinterleaver<S, D, I, O>
where
    S: DemodulatedSymbol,
    D: DeinterleavedSymbol,
    I: CpuBufferReader<Item = S>,
    O: CpuBufferWriter<Item = D>,
{
    pub fn new(ldro: bool, sf: SpreadingFactor) -> Self {
        Self {
            input: I::default(),
            output: O::default(),
            sf: Into::<usize>::into(sf),
            cr: 0,
            is_header: false,
            ldro,
        }
    }
}

trait Deinter<S: DemodulatedSymbol, D: DeinterleavedSymbol>: Send {
    fn deinterleave_block(&mut self, sf_app: usize, cw_len: usize);
}

impl<I, O> Deinter<DemodulatedSymbolHardDecoding, DeinterleavedSymbolHardDecoding>
    for Deinterleaver<DemodulatedSymbolHardDecoding, DeinterleavedSymbolHardDecoding, I, O>
where
    I: CpuBufferReader<Item = DemodulatedSymbolHardDecoding>,
    O: CpuBufferWriter<Item = DeinterleavedSymbolHardDecoding>,
{
    fn deinterleave_block(&mut self, sf_app: usize, cw_len: usize) {
        // Hard-Decoding
        let input = self.input.slice();
        let output = self.output.slice();
        let mut inter_bin: Vec<Vec<bool>> = vec![vec![false; sf_app]; cw_len];
        let mut deinter_bin: Vec<Vec<bool>> = vec![vec![false; cw_len]; sf_app];
        // convert decimal vector to binary vector of vector
        for i in 0..cw_len {
            inter_bin[i] = int2bool(input[i], sf_app);
        }
        // do the actual deinterleaving
        for i in 0..cw_len {
            for j in 0..sf_app {
                deinter_bin[my_modulo(i as isize - j as isize - 1, sf_app)][i] = inter_bin[i][j];
            }
        }
        // transform codewords from binary vector to dec
        for i in 0..sf_app {
            output[i] = bool2int(&deinter_bin[i]) as u8;
        }
    }
}

impl<I, O> Deinter<DemodulatedSymbolSoftDecoding, DeinterleavedSymbolSoftDecoding>
    for Deinterleaver<DemodulatedSymbolSoftDecoding, DeinterleavedSymbolSoftDecoding, I, O>
where
    I: CpuBufferReader<Item = DemodulatedSymbolSoftDecoding>,
    O: CpuBufferWriter<Item = DeinterleavedSymbolSoftDecoding>,
{
    fn deinterleave_block(&mut self, sf_app: usize, cw_len: usize) {
        // wait for a full block to deinterleave
        let input = self.input.slice();
        let output = self.output.slice();
        let mut inter_bin: Vec<[LLR; MAX_SF]> = vec![[0.; MAX_SF]; cw_len];
        let mut deinter_bin: Vec<[LLR; 8]> = vec![[0.; 8]; sf_app];
        for i in 0..cw_len {
            // take only sf_app bits over the sf bits available
            let input_offset = self.sf - sf_app;
            let count = sf_app;
            inter_bin[i][0..count].copy_from_slice(&input[i][input_offset..(input_offset + count)]);
        }
        // Do the actual deinterleaving
        for i in 0..cw_len {
            for j in 0..sf_app {
                deinter_bin[my_modulo(i as isize - j as isize - 1, sf_app)][i] = inter_bin[i][j];
            }
        }
        output[0..sf_app].copy_from_slice(&deinter_bin[0..sf_app]);
        // Write only the cw_len bits over the 8 bits space available
    }
}

impl<S, D, I, O> Kernel for Deinterleaver<S, D, I, O>
where
    S: DemodulatedSymbol,
    D: DeinterleavedSymbol,
    I: CpuBufferReader<Item = S>,
    O: CpuBufferWriter<Item = D>,
    Deinterleaver<S, D, I, O>: Deinter<S, D>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let (input, in_tags) = self.input.slice_with_tags();
        let (output, mut out_tags) = self.output.slice_with_tags();
        let mut input_len = input.len();
        let output_len = output.len();
        // let mut n_input = if self.soft_decoding {
        //     sio.input(0).slice::<[LLR; MAX_SF]>().len()
        // } else {
        //     sio.input(0).slice::<u16>().len()
        // };
        // let n_output = if self.soft_decoding {
        //     sio.output(0).slice::<[LLR; 8]>().len()
        // } else {
        //     sio.output(0).slice::<u8>().len()
        // };

        let tags: Vec<(usize, &HashMap<String, Pmt>)> = in_tags
            .iter()
            .filter_map(|x| match x {
                ItemTag {
                    index,
                    tag: Tag::NamedAny(n, val),
                } => {
                    if n == "frame_info" {
                        match (**val).downcast_ref().unwrap() {
                            Pmt::MapStrPmt(map) => Some((*index, map)),
                            _ => None,
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        let cw_len_current = if self.is_header { 8 } else { self.cr + 4 };

        let tag_tmp = if !tags.is_empty() {
            if tags[0].0 != 0 {
                input_len = tags[0].0;
                if input_len < cw_len_current {
                    warn!("Deinterleaver: incorrect number of samples; dropping.");
                    self.input.consume(input_len);
                    io.call_again = true;
                    return Ok(());
                }
                None
            } else {
                if tags.len() >= 2 {
                    input_len = tags[1].0;
                    if input_len < cw_len_current {
                        warn!("Deinterleaver: too few samples between tags; dropping.");
                        self.input.consume(input_len);
                        io.call_again = true;
                        return Ok(());
                    }
                }
                let (_, tag) = tags[0];
                self.is_header = if let Pmt::Bool(tmp) = tag.get("is_header").unwrap() {
                    *tmp
                } else {
                    panic!()
                };

                if self.is_header {
                    self.sf = if let Pmt::Usize(tmp) = tag.get("sf").unwrap() {
                        *tmp
                    } else {
                        panic!()
                    };
                } else {
                    self.cr = if let Pmt::Usize(tmp) = tag.get("cr").unwrap() {
                        *tmp
                    } else {
                        panic!()
                    };
                    self.ldro = if let Pmt::Bool(tmp) = tag.get("ldro").unwrap() {
                        *tmp
                    } else {
                        panic!()
                    };
                }
                Some(tag.clone())
            }
        } else {
            None
        };

        #[allow(clippy::nonminimal_bool)]
        let sf_app = if (self.sf >= 7 && (self.is_header || self.ldro))
            || (self.sf < 7 && !self.is_header && self.ldro)
        {
            self.sf - 2 // TODO this can be called w/o ever receiving a header tag, causing overflow if sf is not set explicitly in initializer
        } else {
            self.sf
        };
        if output_len < sf_app {
            warn!("[deinterleaver] Not enough output space! {output_len}/{sf_app}");
            return Ok(());
        }
        let cw_len = if self.is_header { 8 } else { self.cr + 4 };

        if input_len >= cw_len {
            if let Some(tag) = tag_tmp {
                out_tags.add_tag(
                    0,
                    Tag::NamedAny("frame_info".to_string(), Box::new(Pmt::MapStrPmt(tag))),
                );
            }
            self.deinterleave_block(sf_app, cw_len);
            self.input.consume(cw_len);
            self.output.produce(sf_app);
        }

        if self.input.finished() && ((input_len < cw_len) || (input_len - cw_len) < cw_len) {
            io.finished = true;
        }
        Ok(())
    }
}
