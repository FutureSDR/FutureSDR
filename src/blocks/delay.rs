use crate::prelude::*;

#[derive(Debug)]
enum State {
    Pad(usize),
    Copy,
    Skip(usize),
}

/// Delays samples.
///
/// # Inputs
///
/// `in`: Stream to delay
///
/// # Outputs
///
/// `out`: Delayed stream
///
/// # Usage
/// ```
/// use futuresdr::blocks::Delay;
/// use futuresdr::runtime::Flowgraph;
/// use num_complex::Complex;
///
/// let mut fg = Flowgraph::new();
///
/// let sink = fg.add_block(Delay::<Complex<f32>>::new(42));
/// ```
#[derive(Block)]
#[message_inputs(new_value)]
pub struct Delay<T, I = DefaultCpuReader<T>, O = DefaultCpuWriter<T>>
where
    T: Copy + Send + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    state: State,
}

impl<T, I, O> Delay<T, I, O>
where
    T: Copy + Send + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    /// Creates a new Dealy block which will delay samples by the specified samples.
    pub fn new(n: isize) -> Self {
        let state = if n > 0 {
            State::Pad(n.try_into().unwrap())
        } else {
            State::Skip((-n).try_into().unwrap())
        };
        Self {
            input: I::default(),
            output: O::default(),
            state,
        }
    }

    async fn new_value(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Pmt::MapStrPmt(new_value) = p {
            let pad: bool = if let Pmt::Bool(temp) = new_value.get("pad").unwrap() {
                *temp
            } else {
                panic!("invalid pad bool")
            };
            let value: usize = if let Pmt::Usize(temp) = new_value.get("value").unwrap() {
                *temp
            } else {
                panic!("invalid value")
            };
            let val = if pad {
                value as isize
            } else {
                -(value as isize)
            };
            let new_val = match self.state {
                State::Pad(n) => n as isize + val,
                State::Skip(n) => -(n as isize) + val,
                State::Copy => val,
            };
            self.state = match new_val.cmp(&0) {
                std::cmp::Ordering::Greater => State::Pad(new_val as usize),
                std::cmp::Ordering::Equal => State::Copy,
                std::cmp::Ordering::Less => State::Skip(new_val.unsigned_abs()),
            }
        } else {
            warn! {"PMT to new_value_handler was not a MapStrPmt"}
        }
        Ok(Pmt::Null)
    }
}

impl<T, I, O> Kernel for Delay<T, I, O>
where
    T: Copy + Send + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();
        let i_len = i.len();
        let o_len = o.len();

        match self.state {
            State::Pad(n) => {
                let m = std::cmp::min(o_len, n);
                o[0..m].fill(unsafe { std::mem::zeroed() });
                self.output.produce(m);

                if m == n {
                    self.state = State::Copy;
                    io.call_again = true;
                    if self.input.finished() {
                        io.finished = true;
                    }
                } else {
                    self.state = State::Pad(n - m);
                }
            }
            State::Skip(n) => {
                let m = std::cmp::min(i_len, n);
                self.input.consume(m);

                if n == m {
                    self.state = State::Copy;
                    io.call_again = true;
                } else {
                    self.state = State::Skip(n - m);
                }
                if self.input.finished() && m == i_len {
                    io.finished = true;
                }
            }
            State::Copy => {
                let m = std::cmp::min(i_len, o_len);
                if m > 0 {
                    o[..m].copy_from_slice(&i[..m]);
                }
                self.input.consume(m);
                self.output.produce(m);
                if self.input.finished() && m == i_len {
                    io.finished = true;
                }
            }
        }
        Ok(())
    }
}
