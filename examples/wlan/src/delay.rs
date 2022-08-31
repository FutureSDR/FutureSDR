use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

#[derive(Debug)]
enum State {
    Pad(usize),
    Copy,
    Skip(usize),
}

pub struct Delay<T: Send + 'static> {
    state: State,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> Delay<T> {
    pub fn new(n: isize) -> Block {
        let state = if n > 0 {
            State::Pad(n.try_into().unwrap())
        } else {
            State::Skip((-n).try_into().unwrap())
        };

        Block::new(
            BlockMetaBuilder::new("Delay").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<T>())
                .add_output("out", std::mem::size_of::<T>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                state,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[async_trait]
impl<T: Send + 'static> Kernel for Delay<T> {
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
                let o = sio.output(0).slice::<u8>();
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
                    unsafe {
                        std::ptr::copy_nonoverlapping(i.as_ptr(), o.as_mut_ptr(), m);
                    }
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
