use std::collections::VecDeque;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::WorkIo;

#[derive(PartialEq, Eq)]
enum State {
    Front(usize, usize),
    Copy(usize),
    Tail(usize),
}

const PADDING: usize = 40000;

pub struct IqDelay {
    state: State,
    buf: VecDeque<f32>,
}

impl IqDelay {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("IQ Delay").build(),
            StreamIoBuilder::new()
                .add_input::<Complex32>("in")
                .add_output::<Complex32>("out")
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                state: State::Tail(0),
                buf: VecDeque::new(),
            },
        )
    }
}

#[async_trait]
impl Kernel for IqDelay {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex32>();
        let o = sio.output(0).slice::<Complex32>();

        let mut consumed = 0;
        let mut produced = 0;

        while produced < o.len() {
            match self.state {
                State::Front(left, size) => {
                    let n = std::cmp::min(o.len() - produced, left);
                    o[produced..produced + n].fill(Complex32::new(0.0, 0.0));
                    produced += n;
                    if n == left {
                        self.state = State::Copy(size);
                        self.buf = VecDeque::from([0.0, 0.0]);
                    } else {
                        self.state = State::Front(left - n, size);
                    }
                }
                State::Copy(left) => {
                    if left == 0 {
                        if let Some(q) = self.buf.pop_front() {
                            o[produced] = Complex32::new(0.0, q);
                            produced += 1;
                        } else {
                            self.state = State::Tail(PADDING);
                        }
                    } else if consumed == i.len() {
                        break;
                    } else {
                        o[produced] = Complex32::new(i[consumed].re, self.buf.pop_front().unwrap());
                        self.buf.push_back(i[consumed].im);
                        produced += 1;
                        consumed += 1;
                        self.state = State::Copy(left - 1);
                    }
                }
                State::Tail(left) => {
                    if left == 0 && consumed == i.len() {
                        break;
                    } else if left == 0 {
                        if let Some(ItemTag {
                            tag: Tag::Id(id), ..
                        }) = sio
                            .input(0)
                            .tags()
                            .iter()
                            .find(|x| x.index == consumed)
                            .cloned()
                        {
                            self.state = State::Front(PADDING, id as usize * 2 * 16 * 4);
                            sio.output(0).add_tag(
                                produced,
                                Tag::NamedUsize(
                                    "burst_start".to_string(),
                                    2 * PADDING + id as usize * 2 * 16 * 4 + 2,
                                ),
                            );
                        } else {
                            panic!("no frame start tag");
                        }
                    } else {
                        let n = std::cmp::min(o.len() - produced, left);
                        o[produced..produced + n].fill(Complex32::new(0.0, 0.0));
                        produced += n;
                        self.state = State::Tail(left - n);
                    }
                }
            }
        }

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);
        if sio.input(0).finished() && consumed == i.len() && self.state == State::Tail(0) {
            io.finished = true;
        }

        Ok(())
    }
}
