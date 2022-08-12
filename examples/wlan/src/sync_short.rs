use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::WorkIo;

const MIN_GAP: usize = 480;
const MAX_SAMPLES: usize = 540 * 80;
const THRESHOLD: f32 = 0.56;

#[derive(Debug)]
enum State {
    Search,
    Found,
    Copy(usize, f32, bool),
}

pub struct SyncShort {
    state: State,
}

impl SyncShort {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("SyncShort").build(),
            StreamIoBuilder::new()
                .add_input("in_sig", std::mem::size_of::<Complex32>())
                .add_input("in_abs", std::mem::size_of::<Complex32>())
                .add_input("in_cor", std::mem::size_of::<f32>())
                .add_output("out", std::mem::size_of::<Complex32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                state: State::Search,
            },
        )
    }
}

#[async_trait]
impl Kernel for SyncShort {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let in_sig = sio.input(0).slice::<Complex32>();
        let in_abs = sio.input(1).slice::<Complex32>();
        let in_cor = sio.input(2).slice::<f32>();
        let out = sio.output(0).slice::<Complex32>();

        let n_input = std::cmp::min(std::cmp::min(in_sig.len(), in_abs.len()), in_cor.len());

        let mut o = 0;
        let mut i = 0;

        while i < n_input && o < out.len() {
            match self.state {
                State::Search => {
                    if in_cor[i] > THRESHOLD {
                        self.state = State::Found;
                    }
                }
                State::Found => {
                    if in_cor[i] > THRESHOLD {
                        let f_offset = -in_abs[i].arg() / 16.0;
                        self.state = State::Copy(0, f_offset, false);
                        sio.output(0)
                            .add_tag(o, Tag::NamedF32("wifi_start".to_string(), f_offset));
                    } else {
                        self.state = State::Search;
                    }
                }
                State::Copy(n_copied, f_offset, mut last_above_threshold) => {
                    if in_cor[i] > THRESHOLD {
                        // resync
                        if last_above_threshold && n_copied > MIN_GAP {
                            let f_offset = -in_abs[i].arg() / 16.0;
                            self.state = State::Copy(0, f_offset, false);
                            sio.output(0)
                                .add_tag(o, Tag::NamedF32("wifi_start".to_string(), f_offset));
                            i += 1;
                            continue;
                        } else {
                            last_above_threshold = true;
                        }
                    } else {
                        last_above_threshold = false;
                    }

                    out[o] = in_sig[i] * Complex32::from_polar(1.0, f_offset * n_copied as f32); // accum?
                    o += 1;

                    if n_copied + 1 == MAX_SAMPLES {
                        self.state = State::Search;
                    } else {
                        self.state = State::Copy(n_copied + 1, f_offset, last_above_threshold);
                    }
                }
            }
            i += 1;
        }

        sio.input(0).consume(i);
        sio.input(1).consume(i);
        sio.input(2).consume(i);
        sio.output(0).produce(o);

        if sio.input(2).finished() && i == in_cor.len() {
            io.finished = true;
        }

        Ok(())
    }
}
