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
    Pad(u64),
    Copy,
    Skip(u64),
}

pub struct Delay<T: Send + 'static> {
    state: State,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> Delay<T> {
    pub fn new(n: i64) -> Block {

        let state = if n > 0 {
            State::Pad(n);
        } else {
            State::Skip(-n as u64);
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
                o[0..m*std::mem::size_of::<T>()].fill(0);
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
            },
            State::Skip(n) => {
                let m = std::cmp::min(i.len(), n);
                sio.input(0).consume(n);

                if m == n {
                    self.state = State::Copy;
                    io.call_again = true;
                } else {
                    self.state = State::Skip(n - m);
                }

                if sio.input(0).finished() {
                    io.finished = true;
                }
            },
            State::Copy => {
                let m = cmp::min(i.len(), o.len());
                if m > 0 {
                    unsafe {
                        ptr::copy_nonoverlapping(i.as_ptr(), o.as_mut_ptr(), m);
                    }
                }
            }
        }
        Ok(())
    }
}
