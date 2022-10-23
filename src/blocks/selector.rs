use std::cmp;
use std::fmt;
use std::ptr;
use std::str::FromStr;

use futures::FutureExt;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropPolicy {
    /// Drop all unselected inputs
    /// Warning: probably your flowgraph at inputs should be rate-limited somehow.
    DropAll,

    /// Drop unselected inputs at the same rate as the selected one.
    /// Warning: probably you will use more CPU than needed,
    /// but get a consistent CPU usage whatever the select
    SameRate,

    /// Do not drop inputs that are unselected.
    NoDrop,
}

impl FromStr for DropPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<DropPolicy, Self::Err> {
        match s {
            "same" => Ok(DropPolicy::SameRate),
            "same-rate" => Ok(DropPolicy::SameRate),
            "SAME" => Ok(DropPolicy::SameRate),
            "SAME_RATE" => Ok(DropPolicy::SameRate),
            "sameRate" => Ok(DropPolicy::SameRate),

            "none" => Ok(DropPolicy::NoDrop),
            "NoDrop" => Ok(DropPolicy::NoDrop),
            "NO_DROP" => Ok(DropPolicy::NoDrop),
            "no-drop" => Ok(DropPolicy::NoDrop),

            "all" => Ok(DropPolicy::DropAll),
            "DropAll" => Ok(DropPolicy::DropAll),
            "drop-all" => Ok(DropPolicy::DropAll),
            "DROP_ALL" => Ok(DropPolicy::DropAll),

            _ => Err("String didn't match value".to_string()),
        }
    }
}

impl fmt::Display for DropPolicy {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            DropPolicy::NoDrop => write!(f, "NoDrop"),
            DropPolicy::DropAll => write!(f, "DropAll"),
            DropPolicy::SameRate => write!(f, "SameRate"),
        }
    }
}

/// Forward the input stream with a given index to the output stream with a
/// given index.
pub struct Selector<A, const N: usize, const M: usize>
where
    A: Send + 'static + Copy,
{
    input_index: usize,
    output_index: usize,
    drop_policy: DropPolicy,
    _p1: std::marker::PhantomData<A>,
}

impl<A, const N: usize, const M: usize> Selector<A, N, M>
where
    A: Send + 'static + Copy,
{
    pub fn new(drop_policy: DropPolicy) -> Block {
        let mut stream_builder = StreamIoBuilder::new();
        for i in 0..N {
            stream_builder = stream_builder.add_input::<A>(format!("in{}", i).as_str());
        }
        for i in 0..M {
            stream_builder = stream_builder.add_output::<A>(format!("out{}", i).as_str());
        }
        Block::new(
            BlockMetaBuilder::new(format!("Selector<{}, {}>", N, M)).build(),
            stream_builder.build(),
            MessageIoBuilder::<Self>::new()
                .add_input(
                    "input_index",
                    |block: &mut Selector<A, N, M>,
                     _mio: &mut MessageIo<Selector<A, N, M>>,
                     _meta: &mut BlockMeta,
                     p: Pmt| {
                        async move {
                            match p {
                                Pmt::U32(v) => block.input_index = (v as usize) % N,
                                Pmt::U64(v) => block.input_index = (v as usize) % N,
                                _ => todo!(),
                            }
                            Ok(Pmt::U32(block.input_index as u32))
                        }
                        .boxed()
                    },
                )
                .add_input(
                    "output_index",
                    |block: &mut Selector<A, N, M>,
                     _mio: &mut MessageIo<Selector<A, N, M>>,
                     _meta: &mut BlockMeta,
                     p: Pmt| {
                        async move {
                            match p {
                                Pmt::U32(v) => block.output_index = (v as usize) % M,
                                Pmt::U64(v) => block.output_index = (v as usize) % M,
                                _ => todo!(),
                            }
                            Ok(Pmt::U32(block.output_index as u32))
                        }
                        .boxed()
                    },
                )
                .build(),
            Selector {
                input_index: 0,
                output_index: 0,
                drop_policy,
                _p1: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<A, const N: usize, const M: usize> Kernel for Selector<A, N, M>
where
    A: Send + 'static + Copy,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(self.input_index).slice_unchecked::<u8>();
        let o = sio.output(self.output_index).slice_unchecked::<u8>();
        let item_size = std::mem::size_of::<A>();

        let m = cmp::min(i.len(), o.len());
        if m > 0 {
            unsafe {
                ptr::copy_nonoverlapping(i.as_ptr(), o.as_mut_ptr(), m);
            }
            //     for (v, r) in i.iter().zip(o.iter_mut()) {
            //         *r = *v;
            //     }

            sio.input(self.input_index).consume(m / item_size);
            sio.output(self.output_index).produce(m / item_size);
        }

        if self.drop_policy != DropPolicy::NoDrop {
            let nb_drop = if self.drop_policy == DropPolicy::SameRate {
                m / item_size // Drop at the same rate as the selected one
            } else {
                std::usize::MAX // Drops all other inputs
            };
            for i in 0..N {
                if i != self.input_index {
                    let input = sio.input(i).slice::<A>();
                    sio.input(i).consume(input.len().min(nb_drop));
                }
            }
        }

        // Maybe this should be configurable behaviour? finish on current finish? when all input have finished?
        if sio.input(self.input_index).finished() && m == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}
