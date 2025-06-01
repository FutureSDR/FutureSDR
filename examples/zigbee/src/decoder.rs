use futuresdr::prelude::*;

const CHIP_MAPPING: [u32; 16] = [
    1618456172, 1309113062, 1826650030, 1724778362, 778887287, 2061946375, 2007919840, 125494990,
    529027475, 838370585, 320833617, 422705285, 1368596360, 85537272, 139563807, 2021988657,
];

fn decode(seq: u32, threshold: u32) -> Option<u8> {
    let mut matches = [32u32; 16];
    for (i, o) in CHIP_MAPPING.iter().zip(matches.iter_mut()) {
        *o = ((seq & 0x7FFFFFFE) ^ (i & 0x7FFFFFFE)).count_ones();
    }
    let (i, v) = matches
        .iter()
        .enumerate()
        .min_by_key(|(_, item)| **item)
        .unwrap();
    if *v < threshold { Some(i as u8) } else { None }
}

#[derive(Debug)]
enum State {
    Search,
    PreambleFound,
    SearchSfd,
    SearchHeader {
        byte: Option<u8>,
    },
    Decode {
        len: usize,
        data: Vec<u8>,
        byte: Option<u8>,
    },
}

struct Correlator {
    shift_reg: u32,
    threshold: u32,
}
impl Correlator {
    fn matching(&self, index: usize) -> bool {
        let ones =
            ((self.shift_reg & 0x7FFFFFFE) ^ (CHIP_MAPPING[index] & 0x7FFFFFFE)).count_ones();
        ones < self.threshold
    }
}

#[derive(Block)]
#[message_outputs(out)]
pub struct Decoder<I = DefaultCpuReader<f32>>
where
    I: CpuBufferReader<Item = f32>,
{
    #[input]
    input: I,
    correlator: Correlator,
    chip_count: u32,
    state: State,
}

impl<I> Decoder<I>
where
    I: CpuBufferReader<Item = f32>,
{
    pub fn new(threshold: u32) -> Self {
        Self {
            input: I::default(),
            correlator: Correlator {
                threshold,
                shift_reg: 0,
            },
            state: State::Search,
            chip_count: 0,
        }
    }
}

impl<I> Kernel for Decoder<I>
where
    I: CpuBufferReader<Item = f32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let inbuf = self.input.slice().to_vec();
        let inbuf_len = inbuf.len();

        for v in inbuf.into_iter() {
            if v > 0.0 {
                self.correlator.shift_reg = (self.correlator.shift_reg << 1) | 1;
            } else {
                self.correlator.shift_reg <<= 1;
            }

            self.chip_count = (self.chip_count + 1) % 32;

            match &mut self.state {
                State::Search => {
                    if self.correlator.matching(0) {
                        // info!("premable found");
                        self.state = State::PreambleFound;
                        self.chip_count = 0;
                    }
                }
                State::PreambleFound => {
                    if self.chip_count == 0 {
                        if self.correlator.matching(7) {
                            self.state = State::SearchSfd;
                        } else if !self.correlator.matching(0) {
                            self.state = State::Search;
                        }
                    }
                }
                State::SearchSfd => {
                    if self.chip_count == 0 {
                        if self.correlator.matching(10) {
                            self.state = State::SearchHeader { byte: None };
                        } else {
                            self.state = State::Search;
                        }
                    }
                }
                State::SearchHeader { byte } => {
                    if self.chip_count == 0 {
                        if let Some(i) =
                            decode(self.correlator.shift_reg, self.correlator.threshold)
                        {
                            if let Some(o) = byte {
                                let len = (i << 4) | *o;
                                if len < 128 {
                                    self.state = State::Decode {
                                        len: (len as usize).saturating_sub(2),
                                        data: Vec::new(),
                                        byte: None,
                                    };
                                } else {
                                    self.state = State::Search;
                                }
                            } else {
                                *byte = Some(i);
                            }
                        } else {
                            self.state = State::Search;
                        }
                    }
                }
                State::Decode { len, data, byte } => {
                    if self.chip_count == 0 {
                        if let Some(i) =
                            decode(self.correlator.shift_reg, self.correlator.threshold)
                        {
                            if let Some(current) = byte {
                                let current = (i << 4) | *current;
                                data.push(current);
                                *byte = None;
                                if data.len() == *len {
                                    // info!("decoded frame");
                                    mio.post("out", Pmt::Blob(std::mem::take(data))).await?;
                                    self.state = State::Search;
                                }
                            } else {
                                *byte = Some(i);
                            }
                        } else {
                            self.state = State::Search;
                        }
                    }
                }
            }
        }

        if self.input.finished() {
            io.finished = true;
        }

        self.input.consume(inbuf_len);

        Ok(())
    }
}
