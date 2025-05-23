use futuresdr::prelude::*;
use std::collections::HashMap;

use crate::utils::*;

#[derive(Block)]
pub struct Deinterleaver<I = circular::Reader<u16>, O = circular::Writer<u8>>
where
    I: CpuBufferReader<Item = u16>,
    O: CpuBufferWriter<Item = u8>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    sf: usize,            // Spreading factor
    cr: usize,            // Coding rate
    is_header: bool, // Indicate that we need to deinterleave the first block with the default header parameters (cr=4/8, reduced rate)
    _soft_decoding: bool, // Hard/Soft decoding
    ldro: bool,      // use low datarate optimization mode
}

impl<I, O> Deinterleaver<I, O>
where
    I: CpuBufferReader<Item = u16>,
    O: CpuBufferWriter<Item = u8>,
{
    pub fn new(soft_decoding: bool) -> Self {
        Self {
            input: I::default(),
            output: O::default(),
            _soft_decoding: soft_decoding,
            sf: 0,
            cr: 0,
            is_header: false,
            ldro: false,
        }
    }
}

impl<I, O> Kernel for Deinterleaver<I, O>
where
    I: CpuBufferReader<Item = u16>,
    O: CpuBufferWriter<Item = u8>,
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
        let sf_app = if (LEGACY_SF_5_6 && (self.is_header || self.ldro))
            || (!LEGACY_SF_5_6
                && ((self.sf >= 7 && (self.is_header || self.ldro))
                    || (self.sf < 7 && !self.is_header && self.ldro)))
        {
            self.sf - 2
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
            // wait for a full block to deinterleave
            // if self.soft_decoding {
            //     let input = sio.input(0).slice::<[LLR; MAX_SF]>();
            //     let output = sio.output(0).slice::<[LLR; 8]>();
            //     let mut inter_bin: Vec<[LLR; MAX_SF]> = vec![[0.; MAX_SF]; cw_len];
            //     let mut deinter_bin: Vec<[LLR; 8]> = vec![[0.; 8]; sf_app];
            //     for i in 0..cw_len {
            //         // take only sf_app bits over the sf bits available
            //         let input_offset = self.sf - sf_app;
            //         let count = sf_app;
            //         inter_bin[i][0..count]
            //             .copy_from_slice(&input[i][input_offset..(input_offset + count)]);
            //     }
            //     // Do the actual deinterleaving
            //     for i in 0..cw_len {
            //         for j in 0..sf_app {
            //             deinter_bin[my_modulo(i as isize - j as isize - 1, sf_app)][i] =
            //                 inter_bin[i][j];
            //         }
            //     }
            //     output[0..sf_app].copy_from_slice(&deinter_bin[0..sf_app]);
            //     // Write only the cw_len bits over the 8 bits space available
            // } else {
            // Hard-Decoding
            let mut inter_bin: Vec<Vec<bool>> = vec![vec![false; sf_app]; cw_len];
            let mut deinter_bin: Vec<Vec<bool>> = vec![vec![false; cw_len]; sf_app];
            // convert decimal vector to binary vector of vector
            for i in 0..cw_len {
                inter_bin[i] = int2bool(input[i], sf_app);
            }
            // do the actual deinterleaving
            for i in 0..cw_len {
                for j in 0..sf_app {
                    deinter_bin[my_modulo(i as isize - j as isize - 1, sf_app)][i] =
                        inter_bin[i][j];
                }
            }
            // transform codewords from binary vector to dec
            for i in 0..sf_app {
                output[i] = bool2int(&deinter_bin[i]) as u8;
            }
            // }
            self.input.consume(cw_len);
            self.output.produce(sf_app);
        }

        if self.input.finished() && ((input_len < cw_len) || (input_len - cw_len) < cw_len) {
            io.finished = true;
        }
        Ok(())
    }
}
