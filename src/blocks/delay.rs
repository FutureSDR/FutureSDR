use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;
use futuresdr_types::Pmt;

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
pub struct Delay<T: Copy + Send + 'static> {
    state: State,
    _type: std::marker::PhantomData<T>,
}

impl<T: Copy + Send + 'static> Delay<T> {
    /// Creates a new Dealy block which will delay samples by the specified samples.
    pub fn new(n: isize) -> TypedBlock<Self> {
        let state = if n > 0 {
            State::Pad(n.try_into().unwrap())
        } else {
            State::Skip((-n).try_into().unwrap())
        };

        TypedBlock::new(
            BlockMetaBuilder::new("Delay").build(),
            StreamIoBuilder::new()
                .add_input::<T>("in")
                .add_output::<T>("out")
                .build(),
            MessageIoBuilder::new()
                .add_input("new_value", Self::new_value_handler)
                .build(),
            Self {
                state,
                _type: std::marker::PhantomData,
            },
        )
    }

    #[message_handler]
    pub fn new_value_handler<'a>(
        &'a mut self,
        _io: &'a mut WorkIo,
        _mio: &'a mut MessageIo<Self>,
        _meta: &'a mut BlockMeta,
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

#[async_trait]
impl<T: Copy + Send + 'static> Kernel for Delay<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();
        let o = sio.output(0).slice::<T>();

        match self.state {
            State::Pad(n) => {
                let m = std::cmp::min(o.len(), n);
                let o = sio.output(0).slice_unchecked::<u8>();
                o[0..m * std::mem::size_of::<T>()].fill(0);
                sio.output(0).produce(m);

                if m == n {
                    self.state = State::Copy;
                    io.call_again = true;
                    if sio.input(0).finished() {
                        io.finished = true;
                    }
                } else {
                    self.state = State::Pad(n - m);
                }
            }
            State::Skip(n) => {
                let m = std::cmp::min(i.len(), n);
                sio.input(0).consume(m);

                if n == m {
                    self.state = State::Copy;
                    io.call_again = true;
                } else {
                    self.state = State::Skip(n - m);
                }

                if sio.input(0).finished() {
                    io.finished = true;
                }
            }
            State::Copy => {
                let m = std::cmp::min(i.len(), o.len());
                if m > 0 {
                    o[..m].copy_from_slice(&i[..m]);
                }
                sio.input(0).consume(m);
                sio.output(0).produce(m);
                if sio.input(0).finished() && m == i.len() {
                    io.finished = true;
                }
            }
        }
        Ok(())
    }
}
