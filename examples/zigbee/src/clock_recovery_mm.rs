use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageOutputs;
use futuresdr::runtime::Result;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::TypedBlock;
use futuresdr::runtime::WorkIo;

#[derive(futuresdr::Block)]
pub struct ClockRecoveryMm {
    omega: f32,
    omega_mid: f32,
    omega_limit: f32,
    gain_omega: f32,
    mu: f32,
    gain_mu: f32,
    last_sample: f32,
    look_ahead: usize,
}

impl ClockRecoveryMm {
    pub fn new(
        omega: f32,
        gain_omega: f32,
        mu: f32,
        gain_mu: f32,
        omega_relative_limit: f32,
    ) -> TypedBlock<Self> {
        TypedBlock::new(
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            Self {
                omega,
                omega_mid: omega,
                omega_limit: omega * omega_relative_limit,
                gain_omega,
                mu,
                gain_mu,
                last_sample: 0.0,
                look_ahead: (omega + omega * omega_relative_limit + gain_mu).ceil() as usize,
            },
        )
    }
}

fn slice(i: f32) -> f32 {
    if i > 0.0 {
        1.0
    } else {
        -1.0
    }
}

impl Kernel for ClockRecoveryMm {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();
        let o = sio.output(0).slice::<f32>();

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

        sio.input(0).consume(ii);
        sio.output(0).produce(oo);

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
