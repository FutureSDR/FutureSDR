use futuresdr::prelude::*;
use std::collections::VecDeque;

#[derive(PartialEq, Eq)]
enum State {
    Front(usize, usize),
    Copy(usize),
    Tail(usize),
}

const PADDING: usize = 40000;

#[derive(Block)]
pub struct IqDelay<I = DefaultCpuReader<Complex32>, O = DefaultCpuWriter<Complex32>>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    state: State,
    buf: VecDeque<f32>,
}

impl<I, O> IqDelay<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    pub fn new() -> Self {
        Self {
            input: I::default(),
            output: O::default(),
            state: State::Tail(0),
            buf: VecDeque::new(),
        }
    }
}

impl<I, O> Default for IqDelay<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<I, O> Kernel for IqDelay<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let (i, tags) = self.input.slice_with_tags();
        let (o, mut otags) = self.output.slice_with_tags();
        let i_len = i.len();

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
                        match tags.iter().find(|x| x.index == consumed).cloned() {
                            Some(ItemTag {
                                tag: Tag::Id(id), ..
                            }) => {
                                self.state = State::Front(PADDING, id as usize * 2 * 16 * 4);
                                otags.add_tag(
                                    produced,
                                    Tag::NamedUsize(
                                        "burst_start".to_string(),
                                        2 * PADDING + id as usize * 2 * 16 * 4 + 2,
                                    ),
                                );
                            }
                            _ => {
                                panic!("no frame start tag");
                            }
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

        self.input.consume(consumed);
        self.output.produce(produced);
        if self.input.finished() && consumed == i_len && self.state == State::Tail(0) {
            io.finished = true;
        }

        Ok(())
    }
}
