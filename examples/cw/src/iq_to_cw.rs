use std::ops::RangeInclusive;
use num_complex::Complex32;
use num_complex::ComplexFloat;
use crate::blocks::cw::CWAlphabet::{self, *};

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;


pub struct IQToCW {
    samples_per_dot: usize,
    avg_power_max: f32,
    avg_power_min: f32,
    sample_count: usize,
    power_before: f32,
    tolerance_per_dot: usize, // Tolerance towards the sending end in sticking to the time slots
    dot_range: RangeInclusive<usize>, // How many samples are still interpreted as a dot
    dash_range: RangeInclusive<usize>,
    letterspace_range: RangeInclusive<usize>,
    wordspace_range: RangeInclusive<usize>,
}

impl IQToCW {
    pub fn new(
        accuracy: usize, // 100 = 100% accuracy = How accurate the timeslots for symbols and between symbols have to be kept
        samples_per_dot: usize,
    ) -> Block {
        let tolerance_per_dot = (samples_per_dot as f32 - ((accuracy as f32 / 100.) * samples_per_dot as f32)) as usize;
        let dot_range = samples_per_dot - tolerance_per_dot..=samples_per_dot + tolerance_per_dot;
        let dash_range = 3 * samples_per_dot - tolerance_per_dot..=3 * samples_per_dot + tolerance_per_dot;
        let letterspace_range = 3 * samples_per_dot - tolerance_per_dot..=3 * samples_per_dot + tolerance_per_dot;
        let wordspace_range = 7 * samples_per_dot - tolerance_per_dot..=7 * samples_per_dot + tolerance_per_dot;

        /*println!("samples per dot: {}", samples_per_dot);
        println!("dot_range: {:?}", dot_range);
        println!("dash_range: {:?}", dash_range);
        println!("letterspace_range: {:?}", letterspace_range);
        println!("wordspace_range: {:?}", wordspace_range);*/

        let block = Block::new(
            BlockMetaBuilder::new("IQToCW").build(),
            StreamIoBuilder::new()
                .add_input::<Complex32>("in")
                .add_output::<CWAlphabet>("out")
                .build(),
            MessageIoBuilder::new().build(),
            IQToCW {
                samples_per_dot: samples_per_dot,
                avg_power_max: 0.,
                avg_power_min: 1.,
                sample_count: 0,
                power_before: 0.,
                tolerance_per_dot: tolerance_per_dot, // // Tolerance towards the sending end in sticking to the time slots
                dot_range: dot_range, // How many samples are still interpreted as a dot
                dash_range: dash_range,
                letterspace_range: letterspace_range,
                wordspace_range: wordspace_range,
            },
        );


        return block;
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for IQToCW {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex32>();
        let o = sio.output(0).slice::<CWAlphabet>();
        if o.is_empty() {
            return Ok(());
        }

        let mut consumed = 0;
        let mut produced = 0;

        let weight = 10.;
        let mut max_avg_max: f32 = 0.;
        for sample in i.iter() {
            let power = (*sample).abs(); //.powi(2);
            let distance = self.avg_power_max - self.avg_power_min;
            max_avg_max = max_avg_max.max(power);

            if power - self.avg_power_min > distance / 2. {
                self.avg_power_max = (weight * power + self.avg_power_max) / (weight + 1.); // Interpret everything as signal, if it cant be classified as noise
            } else {
                self.avg_power_min = (weight * power + self.avg_power_min) / (weight + 1.);
                self.avg_power_max *= 0.99999; // Reduce avg_power_max a little bit again, to detect again weaker signals over time.
            }

            consumed += 1;
        }

        //println!("Total Max: {}, Avg Power Max: {}, Avg Power Min: {}, Threshold: {}", max_avg_max, self.avg_power_max, self.avg_power_min, self.avg_power_max - self.avg_power_min);

        let mut end_of_transmission = true;
        let threshold = (self.avg_power_min + self.avg_power_max) / 2.;

        let mut symbol = None;
        for sample in i.iter() {
            let power = (*sample).abs(); //.powi(2); // Not required

            if (power > threshold) && (self.power_before <= threshold) { // Signal is starting
                match self.sample_count {
                    x if (&self.wordspace_range).contains(&x) => { symbol = Some(WordSpace); } // Wordspace 7 dots (incl tolerance)
                    x if (&self.letterspace_range).contains(&x) => { symbol = Some(LetterSpace); } // Letterspace (Longer than 3 dots (incl tolerance), but shorter than 7 dots (incl tolerance))
                    x if (&self.dot_range).contains(&x) => {} // SymbolSpace (Is a valid symbol)
                    _ => {
                        //println!("Signal pause not a symbol: {} samples", sample_count);
                    }
                }

                //println!("Signal was paused for: {} -> {:?}", sample_count, symbol.or(None));

                self.sample_count = 0;
                end_of_transmission = false;
            }
            if (power <= threshold) && (self.power_before > threshold) { // Signal is stopping
                match self.sample_count {
                    x if (&self.dot_range).contains(&x) => { symbol = Some(Dot); }
                    x if (&self.dash_range).contains(&x) => { symbol = Some(Dash); }
                    _ => {
                        //println!("Signal length not a symbol: {} samples", sample_count);
                    }
                }
                //println!("Signal was present for: {} -> {:?}", sample_count, symbol.or(None));

                self.sample_count = 0;
            }

            if let Some(val) = symbol {
                o[produced] = val;
                produced += 1;
                symbol = None;
            }

            // Special Case: No signal has been received for a longer time than a wordspace needs.
            if self.sample_count > (self.tolerance_per_dot + (7 * self.samples_per_dot)) && (end_of_transmission == false) { // End of transmission
                println!("Transmission ended!");
                end_of_transmission = true;
                o[produced] = LetterSpace;
                o[produced + 1] = WordSpace;
                produced += 2;
            }

            if self.sample_count == usize::MAX { // Dont overflow
                self.sample_count = 0;
            }

            self.sample_count += 1;
            self.power_before = power;
        }

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if sio.input(0).finished() && consumed == i.len() {
            io.finished = true;
        }

        Ok(())
    }
}


pub struct IQToCWBuilder {
    dot_length_ms: f64,
    sample_rate: f64,
    accuracy: usize,
}

impl Default for IQToCWBuilder {
    fn default() -> Self {
        IQToCWBuilder {
            dot_length_ms: 100.,
            sample_rate: 20.,
            accuracy: 90,
        }
    }
}

impl IQToCWBuilder {
    pub fn new() -> IQToCWBuilder {
        IQToCWBuilder::default()
    }

    pub fn dot_length(mut self, dot_length_ms: f64) -> IQToCWBuilder {
        self.dot_length_ms = dot_length_ms;
        self
    }

    pub fn sample_rate(mut self, sample_rate: f64) -> IQToCWBuilder {
        self.sample_rate = sample_rate;
        self
    }

    pub fn accuracy(mut self, accuracy: usize) -> IQToCWBuilder {
        self.accuracy = accuracy;
        self
    }

    pub fn build(self) -> Block {
        IQToCW::new(
            self.accuracy,
            ((self.dot_length_ms / 1000.) * self.sample_rate) as usize,
        )
    }
}
