use std::collections::HashMap;

use futuresdr::macros::async_trait;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Result;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;
use futuresdr::tracing::warn;

use crate::utils::*;

pub struct Deinterleaver {
    sf: usize,           // Spreading factor
    cr: usize,           // Coding rate
    is_header: bool, // Indicate that we need to deinterleave the first block with the default header parameters (cr=4/8, reduced rate)
    soft_decoding: bool, // Hard/Soft decoding
    ldro: bool,      // use low datarate optimization mode
}

impl Deinterleaver {
    pub fn new(soft_decoding: bool) -> TypedBlock<Self> {
        let mut sio = StreamIoBuilder::new();
        if soft_decoding {
            sio = sio.add_input::<[LLR; MAX_SF]>("in");
            sio = sio.add_output::<[LLR; 8]>("out");
        } else {
            sio = sio.add_input::<u16>("in");
            sio = sio.add_output::<u8>("out");
        }
        TypedBlock::new(
            BlockMetaBuilder::new("Deinterleaver").build(),
            sio.build(),
            MessageIoBuilder::new().build(),
            Deinterleaver {
                soft_decoding,
                sf: 0,
                cr: 0,
                is_header: false,
                ldro: false,
            },
        )
    }
}

#[async_trait]
impl Kernel for Deinterleaver {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let mut n_input = if self.soft_decoding {
            sio.input(0).slice::<[LLR; MAX_SF]>().len()
        } else {
            sio.input(0).slice::<u16>().len()
        };
        let n_output = if self.soft_decoding {
            sio.output(0).slice::<[LLR; 8]>().len()
        } else {
            sio.output(0).slice::<u8>().len()
        };

        let tags: Vec<(usize, &HashMap<String, Pmt>)> = sio
            .input(0)
            .tags()
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
                n_input = tags[0].0;
                if n_input < cw_len_current {
                    warn!("Deinterleaver: incorrect number of samples; dropping.");
                    sio.input(0).consume(n_input);
                    io.call_again = true;
                    return Ok(());
                }
                None
            } else {
                if tags.len() >= 2 {
                    n_input = tags[1].0;
                    if n_input < cw_len_current {
                        warn!("Deinterleaver: too few samples between tags; dropping.");
                        sio.input(0).consume(n_input);
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
        if n_output < sf_app {
            warn!(
                "[deinterleaver] Not enough output space! {}/{}",
                n_output, sf_app
            );
            return Ok(());
        }
        let cw_len = if self.is_header { 8 } else { self.cr + 4 };

        if n_input >= cw_len {
            if let Some(tag) = tag_tmp {
                sio.output(0).add_tag(
                    0,
                    Tag::NamedAny("frame_info".to_string(), Box::new(Pmt::MapStrPmt(tag))),
                );
            }
            // wait for a full block to deinterleave
            if self.soft_decoding {
                let input = sio.input(0).slice::<[LLR; MAX_SF]>();
                let output = sio.output(0).slice::<[LLR; 8]>();
                let mut inter_bin: Vec<[LLR; MAX_SF]> = vec![[0.; MAX_SF]; cw_len];
                let mut deinter_bin: Vec<[LLR; 8]> = vec![[0.; 8]; sf_app];
                for i in 0..cw_len {
                    // take only sf_app bits over the sf bits available
                    let input_offset = self.sf - sf_app;
                    let count = sf_app;
                    inter_bin[i][0..count]
                        .copy_from_slice(&input[i][input_offset..(input_offset + count)]);
                }
                // Do the actual deinterleaving
                for i in 0..cw_len {
                    for j in 0..sf_app {
                        deinter_bin[my_modulo(i as isize - j as isize - 1, sf_app)][i] =
                            inter_bin[i][j];
                    }
                }
                output[0..sf_app].copy_from_slice(&deinter_bin[0..sf_app]);
                // Write only the cw_len bits over the 8 bits space available
            } else {
                // Hard-Decoding
                let input = sio.input(0).slice::<u16>();
                let output = sio.output(0).slice::<u8>();
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
            }
            sio.input(0).consume(cw_len);
            sio.output(0).produce(sf_app);
        }

        if sio.input(0).finished() && ((n_input < cw_len) || (n_input - cw_len) < cw_len) {
            io.finished = true;
        }
        Ok(())
    }
}
