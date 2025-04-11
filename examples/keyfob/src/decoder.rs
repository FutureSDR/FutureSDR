use futuresdr::prelude::*;

#[derive(Debug)]
enum State {
    Up(usize),
    Down(usize),
}

#[derive(Block)]
pub struct Decoder<I = circular::Reader<u8>>
where
    I: CpuBufferReader<Item = u8>,
{
    #[input]
    input: I,
    state: State,
    n_read: usize,
    output: bool,
    output_string: String,
}

impl<I> Decoder<I>
where
    I: CpuBufferReader<Item = u8>,
{
    pub fn new() -> Self {
        Self {
            input: I::default(),
            state: State::Down(0),
            n_read: 0,
            output: false,
            output_string: String::new(),
        }
    }

    fn print(mut s: String) {
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
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let inbuf = self.input.slice();
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
                        Self::print(std::mem::take(&mut self.output_string));
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
                        Self::print(std::mem::take(&mut self.output_string));
                    }

                    self.state = State::Down(self.n_read + i);
                }
                _ => {}
            }

            i += 1;
        }

        if self.input.finished() {
            io.finished = true;
        }

        self.input.consume(i);
        self.n_read += i;

        Ok(())
    }
}
