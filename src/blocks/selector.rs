use anyhow::anyhow;
use std::cmp;
use std::fmt;
use std::ptr;
use std::str::FromStr;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Pmt;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Drop Policy for [`Selector`] block
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
    /// Create Selector block
    pub fn new(drop_policy: DropPolicy) -> TypedBlock<Self> {
        let mut stream_builder = StreamIoBuilder::new();
        for i in 0..N {
            stream_builder = stream_builder.add_input::<A>(format!("in{i}").as_str());
        }
        for i in 0..M {
            stream_builder = stream_builder.add_output::<A>(format!("out{i}").as_str());
        }
        TypedBlock::new(
            BlockMetaBuilder::new(format!("Selector<{N}, {M}>")).build(),
            stream_builder.build(),
            MessageIoBuilder::<Self>::new()
                .add_input("input_index", Self::input_index)
                .add_input("output_index", Self::output_index)
                .build(),
            Selector {
                input_index: 0,
                output_index: 0,
                drop_policy,
                _p1: std::marker::PhantomData,
            },
        )
    }

    fn pmt_to_index(p: &Pmt) -> Result<Option<usize>> {
        match p {
            Pmt::U32(v) => Ok(Some(*v as usize % N)),
            Pmt::U64(v) => Ok(Some(*v as usize % N)),
            Pmt::Usize(v) => Ok(Some(*v % N)),
            Pmt::Finished | Pmt::Ok => Ok(None),
            o => Err(anyhow!("Invalid index specification: {:?}", o)),
        }
    }

    #[message_handler]
    async fn input_index(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Some(i) = Self::pmt_to_index(&p)? {
            self.input_index = i;
        }
        Ok(Pmt::U32(self.input_index as u32))
    }

    #[message_handler]
    async fn output_index(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Some(i) = Self::pmt_to_index(&p)? {
            self.output_index = i;
        }
        Ok(Pmt::U32(self.output_index as u32))
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
                usize::MAX // Drops all other inputs
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
