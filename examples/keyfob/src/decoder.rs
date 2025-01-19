use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageOutputs;
use futuresdr::runtime::Result;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;
use futuresdr::tracing::info;

#[derive(Debug)]
enum State {
    Up(usize),
    Down(usize),
}

#[derive(futuresdr::Block)]
pub struct Decoder {
    state: State,
    n_read: usize,
    output: bool,
    output_string: String,
}

impl Decoder {
    pub fn new() -> TypedBlock<Self> {
        TypedBlock::new(
            StreamIoBuilder::new().add_input::<u8>("in").build(),
            Self {
                state: State::Down(0),
                n_read: 0,
                output: false,
                output_string: String::new(),
            },
        )
    }

    fn print(&mut self) {
        let mut s = std::mem::take(&mut self.output_string);
        let offset = s.find("10101111").unwrap_or(s.len());
        s.replace_range(..offset, "");

        let l = s.len();
        if l >= 8 {
            if s.ends_with("11010101") {
                s += " (Close)";
            } else if s.ends_with("11100011") {
                s += " (Open)";
            } else if s.ends_with("10111001") {
                s += " (Trunk)";
            }

            info!("RXed {}", s);
        }
    }
}

impl Kernel for Decoder {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let inbuf = sio.input(0).slice::<u8>();
        let mut i = 0;

        while i < inbuf.len() {
            match (&self.state, inbuf[i]) {
                (State::Down(since), 1) => {
                    let diff = self.n_read + i - since;
                    if (63..=83).contains(&diff) {
                        if !self.output {
                            self.output = true;
                        } else {
                            self.output = false;
                            self.output_string += "0";
                        }
                    } else if (131..=161).contains(&diff) {
                        self.output = false;
                        self.output_string += "0";
                    } else {
                        self.print()
                    }

                    self.state = State::Up(self.n_read + i);
                }
                (State::Up(since), 0) => {
                    let diff = self.n_read + i - since;
                    if (63..=83).contains(&diff) {
                        if !self.output {
                            self.output = true;
                        } else {
                            self.output = false;
                            self.output_string += "1";
                        }
                    } else if (131..=161).contains(&diff) {
                        self.output = false;
                        self.output_string += "1";
                    } else {
                        self.print()
                    }

                    self.state = State::Down(self.n_read + i);
                }
                _ => {}
            }

            i += 1;
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        sio.input(0).consume(i);
        self.n_read += i;

        Ok(())
    }
}
