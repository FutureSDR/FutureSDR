use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

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
    if *v < threshold {
        Some(i as u8)
    } else {
        None
    }
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

pub struct Decoder {
    chip_count: u32,
    shift_reg: u32,
    threshold: u32,
    state: State,
}

impl Decoder {
    pub fn new(threshold: u32) -> Block {
        Block::new(
            BlockMetaBuilder::new("Decoder").build(),
            StreamIoBuilder::new().add_input::<f32>("in").build(),
            MessageIoBuilder::<Self>::new().add_output("out").build(),
            Self {
                threshold,
                state: State::Search,
                shift_reg: 0,
                chip_count: 0,
            },
        )
    }

    fn matching(&self, index: usize) -> bool {
        let ones =
            ((self.shift_reg & 0x7FFFFFFE) ^ (CHIP_MAPPING[index] & 0x7FFFFFFE)).count_ones();
        ones < self.threshold
    }
}

#[async_trait]
impl Kernel for Decoder {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let inbuf = sio.input(0).slice::<f32>();
        let mut i = 0;

        while i < inbuf.len() {
            if inbuf[i] > 0.0 {
                self.shift_reg = (self.shift_reg << 1) | 1;
            } else {
                self.shift_reg <<= 1;
            }

            self.chip_count = (self.chip_count + 1) % 32;

            match &mut self.state {
                State::Search => {
                    if self.matching(0) {
                        // info!("premable found");
                        self.state = State::PreambleFound;
                        self.chip_count = 0;
                    }
                }
                State::PreambleFound => {
                    if self.chip_count == 0 {
                        if self.matching(7) {
                            self.state = State::SearchSfd;
                        } else if !self.matching(0) {
                            self.state = State::Search;
                        }
                    }
                }
                State::SearchSfd => {
                    if self.chip_count == 0 {
                        if self.matching(10) {
                            self.state = State::SearchHeader { byte: None };
                        } else {
                            self.state = State::Search;
                        }
                    }
                }
                State::SearchHeader { byte } => {
                    if self.chip_count == 0 {
                        if let Some(i) = decode(self.shift_reg, self.threshold) {
                            if let Some(o) = byte {
                                let len = (i << 4) | *o;
                                if len < 128 {
                                    self.state = State::Decode {
                                        len: len as usize,
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
                        if let Some(i) = decode(self.shift_reg, self.threshold) {
                            if let Some(current) = byte {
                                let current = (i << 4) | *current;
                                data.push(current);
                                *byte = None;
                                if data.len() == *len {
                                    // info!("decoded frame");
                                    mio.post(0, Pmt::Blob(std::mem::take(data))).await;
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

            i += 1;
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        sio.input(0).consume(i);

        Ok(())
    }
}
