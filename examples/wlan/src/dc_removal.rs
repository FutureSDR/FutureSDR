use futuresdr::runtime::dev::prelude::*;

/// Remove complex DC offset using an exponential moving average.
///
/// Tracks I and Q independently with coefficient 1e-5, preserving stream tags.
/// Connect `input` to uncorrected IQ and consume corrected IQ from `output`.
#[derive(Block)]
pub struct DcRemoval<IN = DefaultCpuReader<Complex32>, OUT = DefaultCpuWriter<Complex32>>
where
    IN: CpuBufferReader<Item = Complex32>,
    OUT: CpuBufferWriter<Item = Complex32>,
{
    #[input]
    input: IN,
    #[output]
    output: OUT,
    average: Complex32,
}

impl DcRemoval {
    /// Create a DC removal filter with a tracking coefficient of 1e-5.
    pub fn new() -> Self {
        Self::with_buffers()
    }
}

impl<IN: CpuBufferReader<Item = Complex32>, OUT: CpuBufferWriter<Item = Complex32>>
    DcRemoval<IN, OUT>
{
    /// Create a DC removal filter with custom stream buffers.
    pub fn with_buffers() -> Self {
        Self {
            input: Default::default(),
            output: Default::default(),
            average: Complex32::new(0.0, 0.0),
        }
    }
}

impl<IN: CpuBufferReader<Item = Complex32>, OUT: CpuBufferWriter<Item = Complex32>> Kernel
    for DcRemoval<IN, OUT>
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let (input, tags) = self.input.slice_with_tags();
        let (output, mut output_tags) = self.output.slice_with_tags();
        let input_len = input.len();
        let n = input_len.min(output.len());
        for (sample, out) in input.iter().zip(output.iter_mut()) {
            self.average += (*sample - self.average) * 1.0e-5;
            *out = *sample - self.average;
        }
        for tag in tags.iter().filter(|tag| tag.index < n) {
            output_tags.add_tag(tag.index, tag.tag.clone());
        }
        self.input.consume(n);
        self.output.produce(n);
        if self.input.finished() && n == input_len {
            io.finished = true;
        }
        Ok(())
    }
}

impl<IN: CpuBufferReader<Item = Complex32>, OUT: CpuBufferWriter<Item = Complex32>> Default
    for DcRemoval<IN, OUT>
{
    fn default() -> Self {
        Self::with_buffers()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use futuresdr::runtime::mocker::Mocker;
    use futuresdr::runtime::mocker::Reader;
    use futuresdr::runtime::mocker::Writer;

    #[test]
    fn removes_complex_offset_and_preserves_ac_across_calls() {
        let mut dc = Mocker::new(DcRemoval::<Reader<Complex32>, Writer<Complex32>>::with_buffers());
        let offset = Complex32::new(0.5, -0.25);
        dc.input().set(vec![offset; 600_000]);
        dc.output().reserve(600_000);
        dc.run();
        let (samples, _) = dc.output().get();
        assert!(samples.last().unwrap().norm() < 0.003);
        let ac = Complex32::new(0.2, 0.1);
        dc.input().set(
            (0..1000)
                .map(|i| offset + if i % 2 == 0 { ac } else { -ac })
                .collect(),
        );
        dc.output().reserve(1000);
        dc.run();
        let (samples, _) = dc.output().get();
        for (i, sample) in samples.iter().enumerate() {
            let expected = if i % 2 == 0 { ac } else { -ac };
            assert!((*sample - expected).norm() < 0.003);
        }
    }
}
