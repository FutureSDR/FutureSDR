use anyhow::anyhow;
use std::array::from_fn;
use std::cmp;
use std::fmt;
use std::str::FromStr;

use crate::prelude::*;

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
#[derive(Block)]
#[message_inputs(input_index, output_index)]
pub struct Selector<
    A,
    const N: usize,
    const M: usize,
    IN = DefaultCpuReader<A>,
    OUT = DefaultCpuWriter<A>,
> where
    A: Send + 'static + Copy,
    IN: CpuBufferReader<Item = A>,
    OUT: CpuBufferWriter<Item = A>,
{
    #[input]
    inputs: [IN; N],
    #[output]
    outputs: [OUT; M],
    input_index: usize,
    output_index: usize,
    drop_policy: DropPolicy,
}

impl<A, const N: usize, const M: usize, IN, OUT> Selector<A, N, M, IN, OUT>
where
    A: Send + 'static + Copy,
    IN: CpuBufferReader<Item = A>,
    OUT: CpuBufferWriter<Item = A>,
{
    /// Create Selector block
    pub fn new(drop_policy: DropPolicy) -> Self {
        Selector {
            inputs: from_fn(|_| IN::default()),
            outputs: from_fn(|_| OUT::default()),
            input_index: 0,
            output_index: 0,
            drop_policy,
        }
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

    async fn input_index(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        if let Some(i) = Self::pmt_to_index(&p)? {
            self.input_index = i;
        }
        Ok(Pmt::U32(self.input_index as u32))
    }

    async fn output_index(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
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
impl<A, const N: usize, const M: usize, IN, OUT> Kernel for Selector<A, N, M, IN, OUT>
where
    A: Send + 'static + Copy,
    IN: CpuBufferReader<Item = A>,
    OUT: CpuBufferWriter<Item = A>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.inputs[self.input_index].slice();
        let o = self.outputs[self.output_index].slice();

        let i_len = i.len();
        let m = cmp::min(i_len, o.len());
        for (v, r) in i.iter().zip(o.iter_mut()) {
            *r = *v;
        }

        self.inputs[self.input_index].consume(m);
        self.outputs[self.output_index].produce(m);

        if self.drop_policy != DropPolicy::NoDrop {
            let nb_drop = if self.drop_policy == DropPolicy::SameRate {
                m // Drop at the same rate as the selected one
            } else {
                usize::MAX // Drops all other inputs
            };
            for i in 0..N {
                if i != self.input_index {
                    let l = self.inputs[i].slice().len();
                    self.inputs[i].consume(l.min(nb_drop));
                }
            }
        }

        if self.inputs[self.input_index].finished() && m == i_len {
            io.finished = true;
        }

        Ok(())
    }
}
