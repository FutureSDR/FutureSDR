use futuresdr::prelude::*;

#[derive(Block)]
pub struct ClockRecoveryMm<I = DefaultCpuReader<f32>, O = DefaultCpuWriter<f32>>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    omega: f32,
    omega_mid: f32,
    omega_limit: f32,
    gain_omega: f32,
    mu: f32,
    gain_mu: f32,
    last_sample: f32,
    look_ahead: usize,
}

impl<I, O> ClockRecoveryMm<I, O>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    pub fn new(
        omega: f32,
        gain_omega: f32,
        mu: f32,
        gain_mu: f32,
        omega_relative_limit: f32,
    ) -> Self {
        let look_ahead = (omega + omega * omega_relative_limit + gain_mu).ceil() as usize;
        let mut input = I::default();
        input.set_min_items(look_ahead + 1);
        Self {
            input,
            output: O::default(),
            omega,
            omega_mid: omega,
            omega_limit: omega * omega_relative_limit,
            gain_omega,
            mu,
            gain_mu,
            last_sample: 0.0,
            look_ahead,
        }
    }
}

fn slice(i: f32) -> f32 {
    if i > 0.0 { 1.0 } else { -1.0 }
}

impl<I, O> Kernel for ClockRecoveryMm<I, O>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();

        let mut ii = 0;
        let mut oo = 0;

        while ii + self.look_ahead < i.len() && oo < o.len() {
            o[oo] = i[ii] + self.mu * (i[ii + 1] - i[ii]);
            let mm_val = slice(self.last_sample) * o[oo] - slice(o[oo]) * self.last_sample;
            self.last_sample = o[oo];

            self.omega += self.gain_omega * mm_val;
            self.omega = self.omega_mid
                + (self.omega - self.omega_mid).clamp(-self.omega_limit, self.omega_limit);
            self.mu += self.omega + self.gain_mu * mm_val;

            ii += self.mu.floor() as usize;
            self.mu -= self.mu.floor();
            oo += 1;
        }

        self.input.consume(ii);
        self.output.produce(oo);

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
