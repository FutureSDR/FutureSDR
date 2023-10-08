use futuresdr::anyhow::Result;
use futuresdr::macros::async_trait;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;
use std::collections::VecDeque;

struct TimingErrorDetector {
    input: VecDeque<Complex32>,
    error: f32,
    error_depth: usize,
    input_clock: i32,
    inputs_per_symbol: usize,
    needs_derivative: bool,
    needs_lookahead: bool,
    prev_error: f32,
}

impl TimingErrorDetector {

    fn new(inputs_per_symbol: usize, error_depth: usize, needs_lookahead: bool, needs_derivative: bool) -> Self {
        let mut s = Self {
            error: 0.0,
            error_depth,
            input: VecDeque::new(),
            input_clock: 0,
            inputs_per_symbol,
            needs_derivative,
            needs_lookahead,
            prev_error: 0.0,
        };
        s.sync_reset();
        s
    }

    fn inputs_per_symbol(&self) -> usize {
        self.inputs_per_symbol
    }

    fn input(&mut self, x: f32, dx: f32) {
        self.input.push_front(Complex32::new(x, 0.0));
        self.input.pop_back();
        assert_eq!(self.needs_derivative, false);
        assert_eq!(self.needs_lookahead, false);


        self.advance_input_clock();
        if self.input_clock == 0 {
            self.prev_error = self.error;
            self.error = self.compute_error();
        }
    }

    fn needs_lookahead(&self) -> bool {
        self.needs_lookahead
    }

    fn input_lookahead(&mut self, x: f32, dx: f32) {
        assert_eq!(self.needs_lookahead, false);
        // do not need lookahead
    }

    fn needs_derivative(&self) -> bool {
        self.needs_derivative
    }

    fn error(&self) -> f32 {
        self.error
    }

    fn revert(&mut self, preserve_error: bool) {
        if (self.input_clock == 0) && (preserve_error == false) {
            self.error = self.prev_error; 
        }
        self.revert_input_clock();

        assert_eq!(self.needs_derivative, false);

        self.input.push_back(*self.input.back().unwrap());
        self.input.pop_front();
    }

    fn sync_reset(&mut self) {
        self.error = 0.0;
        self.prev_error = 0.0;

        self.input = VecDeque::from_iter(vec![Complex32::new(0.0, 0.0); self.error_depth].into_iter());
        self.sync_reset_input_clock();
    }

    fn advance_input_clock(&mut self) {
        self.input_clock = (self.input_clock + 1) % self.inputs_per_symbol as i32;
    }

    fn revert_input_clock(&mut self)
    {
        if self.input_clock == 0 {
            self.input_clock = self.inputs_per_symbol as i32 - 1;
        } else {
            self.input_clock -= 1;
        }
    }

    fn sync_reset_input_clock(&mut self) {
        self.input_clock = self.inputs_per_symbol as i32 - 1;
    }

    fn compute_error(&self) -> f32 {
        (self.input[2].re - self.input[0].re) * self.input[1].re
    }
}

pub struct SymbolSync {
    ted: TimingErrorDetector,
}

impl SymbolSync {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("SymbolSync").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                ted: TimingErrorDetector::new(2, 3, false, false),
            },
        )
    }
}

#[async_trait]
impl Kernel for SymbolSync {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<f32>();
        let out = sio.output(0).slice::<f32>();

        io.finished = true;

        Ok(())
    }
}
