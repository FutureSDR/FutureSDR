use num_complex::Complex32;
//use std::iter::{Chain, Take, Repeat, repeat};
use crate::blocks::cw::{CWAlphabet};

use crate::anyhow::Result;
use crate::blocks::cw::CWAlphabet::LetterSpace;
use crate::blocks::cw::SmoothSymbol;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;


pub struct CWToIQ {
    samples_per_dot: usize,
    transition_smoothness: f32,
    current_it: SmoothSymbol, //Box<dyn std::iter::Iterator<Item = f32> + Send>, //Chain<Take<Repeat<f32>>, Take<Repeat<f32>>>,
}

/*fn get_iterator(symbol: &CWAlphabet, samples_per_dot: usize) -> Chain<Take<Repeat<f32>>, Take<Repeat<f32>>> {
    match symbol {
        Dot => std::iter::repeat(1.0)
            .take(samples_per_dot)
            .chain(std::iter::repeat(0.0).take(samples_per_dot)),
        Dash => std::iter::repeat(1.0)
            .take(3 * samples_per_dot)
            .chain(std::iter::repeat(0.0).take(samples_per_dot)),
        LetterSpace => std::iter::repeat(0.0)
            .take(2 * samples_per_dot)
            .chain(std::iter::repeat(0.0).take(0)),
        WordSpace => std::iter::repeat(0.0)
            .take(2 * samples_per_dot)
            .chain(std::iter::repeat(0.0).take(0)),
    }
}*/

impl CWToIQ {
    pub fn new(
        samples_per_dot: f64,
        transition_smoothness: f32
    ) -> Block {
        if samples_per_dot < 1. {
            error!("Invalid ratio of samples per dot: {}! (dot_length_ms / 1000.) * sample_rate must not be less than 1", samples_per_dot);
        }

        Block::new(
            BlockMetaBuilder::new("CWToIQ").build(),
            StreamIoBuilder::new()
                .add_input::<CWAlphabet>("in")
                .add_output::<Complex32>("out")
                .build(),
            MessageIoBuilder::new().build(),
            CWToIQ {
                samples_per_dot: samples_per_dot as usize,
                transition_smoothness: transition_smoothness,
                current_it: SmoothSymbol::new(LetterSpace, 0., 0), //Box::new(std::iter::empty()), //repeat(0.0).take(0).chain(repeat(0.0).take(0)),
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for CWToIQ {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<CWAlphabet>();
        let o = sio.output(0).slice::<Complex32>();
        if o.is_empty() {
            return Ok(());
        }

        let mut i_iter = i.iter();
        let mut consumed = 0;
        let mut produced = 0;

        while produced < o.len() { // As long as this block produced less bytes than the output buffer can hold, we proceed to fill it.
            let buffer_offset = produced; // Get an offset into our output buffer in case it is already partially filled with data from previous iteration.
            self.current_it
                .by_ref()
                .take(o.len() - produced)
                .enumerate()
                .map(|(idx, data)| { o[idx + buffer_offset] = Complex32::new(data, data) })
                .for_each(|_| produced += 1); // check if the current iterator still has data in it and consume it.

            if produced < o.len() { // there is still space left in the output buffer, even after reading all values from the current iterator.
                if let Some(symbol) = i_iter.next() { // Get the next symbol from the input buffer
                    self.current_it = SmoothSymbol::new(*symbol, self.transition_smoothness, self.samples_per_dot);  //get_iterator(symbol, self.samples_per_dot); // Get an iterator over floating point numbers, representing a symbol of our input buffer
                    consumed += 1;
                } else { // There are no more symbols left in our input buffer. We processed them all. Leave the while loop by breaking.
                    break; // iterator is now empty -> we processed all symbols -> leave while loop
                }
            } else { //our output buffer is full. we have to flush it, before we continue
                io.call_again = true;
            }
        }

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && consumed == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}


pub struct CWToIQBuilder {
    dot_length_ms: f64,
    sample_rate: f64,
    transition_smoothness: f32,
}

impl Default for CWToIQBuilder {
    fn default() -> Self {
        CWToIQBuilder {
            dot_length_ms: 100.,
            sample_rate: 20.,
            transition_smoothness: 5., // in percent
        }
    }
}

impl CWToIQBuilder {
    pub fn new() -> CWToIQBuilder {
        CWToIQBuilder::default()
    }

    pub fn dot_length(mut self, dot_length_ms: f64) -> CWToIQBuilder {
        self.dot_length_ms = dot_length_ms;
        self
    }

    pub fn sample_rate(mut self, sample_rate: f64) -> CWToIQBuilder {
        self.sample_rate = sample_rate;
        self
    }

    pub fn smoothness(mut self, transition_smoothness: f32) -> CWToIQBuilder {
        self.transition_smoothness = transition_smoothness;
        self
    }

    pub fn build(self) -> Block {
        CWToIQ::new(
            (self.dot_length_ms / 1000.) * self.sample_rate,
            self.transition_smoothness,
        )
    }
}
