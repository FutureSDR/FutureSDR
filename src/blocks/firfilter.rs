use crate::runtime::{Block, StreamIoBuilder, BlockMetaBuilder, MessageIoBuilder, AsyncKernel, WorkIo, StreamIo, MessageIo, BlockMeta};
use std::{mem, sync::Arc};
use num_complex::Complex;
use async_trait::async_trait;
use rustfft::{FftPlanner, Fft};
use anyhow::Result;

pub enum WindowType {
    None,
    Blackman,
    Hamming
}

// pub struct Point<T> where
//     T: Sized + Sync + Send {
//     x: T,
//     y: T
// }

pub enum FIRFilterResponseShape {
    LowPass(f32),
    HighPass(f32),
    BandPass(f32, f32)
//    Custom(Vec<Point<f32>>)
}

pub struct FIRFilter {
    window_type: WindowType,
    fft_width: usize,
    impulse_size: usize,
    response_shape: Vec<f32>,
    impulse_shape: Vec<f32>,
    impulse_fft: Vec<Complex<f32>>,
    forward_fft: Arc<dyn Fft<f32>>,
    inverse_fft: Arc<dyn Fft<f32>>    
}

impl FIRFilter {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f32, fft_width: usize, impulse_size: usize, response_shape: Vec<f32>, window_type: WindowType) -> Block {
        let mut planner = FftPlanner::new();

        Block::new_async(
            BlockMetaBuilder::new("FIRFilter").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<Complex<f32>>())
                .add_output("out", mem::size_of::<Complex<f32>>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                window_type,
                fft_width,
                impulse_size,
                response_shape,
                impulse_shape: vec![0.0; fft_width],
                impulse_fft: vec![Complex::new(0.0, 0.0); fft_width],
                forward_fft: planner.plan_fft_forward(fft_width),
                inverse_fft: planner.plan_fft_inverse(fft_width)                
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for FIRFilter {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex<f32>>();
        let o = sio.output(0).slice::<Complex<f32>>();

        let n = std::cmp::min(i.len(), o.len());

//        let impulse = self.impulse_shape.iter().map(|v| Complex::new(*v, 0.0_f32)).collect::<Vec<Complex<f32>>>();
 
        let n_groups = n / self.fft_width;
        println!("Running {} groups", n_groups);
        let mut processed = 0_usize;
        println!("FIR: Running {} groups from {} data (n={})", n_groups, i.len(), n);
        for index in 0..n_groups {
            println!("FIR: Running Filter Loop");
            
            let start_index = index * self.fft_width;
//            let stop_index = start_index + self.fft_width;

            // Real Component
            let mut real = vec![Complex::new(0_f32, 0_f32); self.fft_width];
            for ind in 0..self.fft_width {
                real[ind] = Complex::new(i[ind+start_index].re, 0_f32);
            }
            self.forward_fft.process(&mut real);
            real = real.iter().zip(self.impulse_fft.iter()).map(|(a, b)| a*b).collect::<Vec<Complex<f32>>>();
            self.inverse_fft.process(&mut real);

            // Imaginary Component
            let mut imag = vec![Complex::new(0_f32, 0_f32); self.fft_width];
            for ind in 0..self.fft_width {
                imag[ind] = Complex::new(i[ind+start_index].im, 0_f32);
            }
            self.forward_fft.process(&mut imag);
            imag = imag.iter().zip(self.impulse_fft.iter()).map(|(a, b)| a*b).collect::<Vec<Complex<f32>>>();
            self.inverse_fft.process(&mut imag);

            // Put them together and normalize the result
            let tmp = real
                .iter()
                .zip(imag.iter())
                .map(|(a, b)| Complex::new(a.re/self.fft_width as f32, b.re/self.fft_width as f32))
                .collect::<Vec<Complex<f32>>>();

            // Save the result
            for ind in 0..self.fft_width {
                o[ind+start_index] = tmp[ind];
            }
            
            processed = processed + self.fft_width;
        }

        println!("FIR: Finished: {}", sio.input(0).finished());

        if sio.input(0).finished() && ((i.len() - processed) < self.fft_width) {
            println!("FIR: Marking as done");
            io.finished = true;
        }

        if n == 0 {
            println!("FIR: Exiting early... with processed of {}", processed);
            return Ok(());
        }

        sio.input(0).consume(processed);
        sio.output(0).produce(processed);

        println!("FIR: Finished... (Processed: {})", processed);

        Ok(())
    }

    async fn init(
        &mut self,
        _sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {

        // Calculate the window function
        let mut window = match self.window_type {
            WindowType::None => {
                vec![1.0_f32; self.fft_width]
            },
            WindowType::Blackman => {
                let mult = std::f32::consts::PI / self.impulse_size as f32;
                (0..self.impulse_size).map(|m| 0.42_f32 - 0.5*f32::cos(2.0*mult * m as f32) + 0.08*f32::cos(4.0*mult * m as f32)).collect::<Vec<f32>>()
            },
            WindowType::Hamming => {
                let mult = std::f32::consts::PI / self.impulse_size as f32;
                (0..=self.impulse_size).map(|m| 0.54_f32 - 0.46*f32::cos(2.0*mult * m as f32)).collect::<Vec<f32>>()
            }
        };

        // Pad the window function to the FFT size
        window.append(&mut vec![0.0_f32; self.fft_width - self.impulse_size]);

        // Generate the response instance

        //// Inverse FFT
        let mut tmp = self.response_shape.iter().map(|m| { Complex::new(*m, 0.0_f32) }).collect::<Vec<Complex<f32>>>();
        self.inverse_fft.process(tmp.as_mut_slice());

        //// Extract the Real component and normalize
        let mut impulse = tmp.iter().map(|m| m.re / self.fft_width as f32).collect::<Vec<f32>>();
//        println!("{:?}", &impulse[0..self.fft_width]);

        //// Shift and Truncate
        //// There must be a cleaner and better way to do this...  (had to do this in a pinch...)
        let mut tmp: Vec<f32> = vec![0.0_f32; self.fft_width];
        for i in 0..self.impulse_size as i32 {
            let mut src_index = i - (self.impulse_size as f32/2.0) as i32;
            if src_index < 0 {
                src_index = src_index + self.fft_width as i32;
            }

            tmp[i as usize] = impulse[src_index as usize];
        };
        impulse = tmp.clone();

        //// Window
        let mut impulse = impulse.iter().zip(window.iter()).map(|(a, b)| a*b).collect::<Vec<f32>>();

        //// Normalize the window function
        let norm: f32 = impulse.iter().sum();
        impulse = impulse.iter().map(|i| i/norm).collect::<Vec<f32>>();

        self.impulse_shape = impulse.clone();
        
        //// Create the FFT of the impulse for the convolution
        let mut impulse_fft = self.impulse_shape.clone().iter().map(|m| Complex::new(*m, 0_f32)).collect::<Vec<Complex<f32>>>();
        self.forward_fft.process(impulse_fft.as_mut_slice());    

        self.impulse_fft = impulse_fft.clone();

        Ok(())
    }

}

pub struct FIRFilterBuilder {
    window_type: WindowType,
    fft_width: usize,
    impulse_size: usize,
    response_type: Option<FIRFilterResponseShape>,
    sample_rate: Option<f32>
}

impl FIRFilterBuilder {
    pub fn new() -> FIRFilterBuilder {
        FIRFilterBuilder {
            window_type: WindowType::Blackman,
            fft_width: 1024,
            impulse_size: 100,
            response_type: None,
            sample_rate: None
        }
    }

    pub fn build(self) -> Block {

        if self.response_type.is_none() {
            println!("Response Type not defined for FIR filter")
        }

        // Create a shape here
        let response_shape = match self.response_type.unwrap() {
            FIRFilterResponseShape::LowPass(threshold) => {
                let index = ((self.fft_width as f32) * (threshold / self.sample_rate.unwrap())).floor() as usize;
                let mut tmp = vec![0.0_f32; self.fft_width];
                for i in 0..index {
                    tmp[i] = 1.0_f32;
                }
                tmp
            },
            FIRFilterResponseShape::HighPass(threshold) => {
                let index = ((self.fft_width as f32) * (threshold / self.sample_rate.unwrap())).floor() as usize;
                let mut tmp = vec![0.0_f32; self.fft_width];
                for i in index..self.fft_width {
                    tmp[i] = 1.0_f32;
                }
                tmp
            },
            FIRFilterResponseShape::BandPass(low, high) => {
                let index_low = ((self.fft_width as f32) * (low / self.sample_rate.unwrap())).floor() as usize;
                let index_high = ((self.fft_width as f32) * (high / self.sample_rate.unwrap())).floor() as usize;
                let mut tmp = vec![0.0_f32; self.fft_width];
                for i in index_low..index_high {
                    tmp[i] = 1.0_f32;
                }
                tmp
            }
        };

//        println!("{:?}", response_shape);

        FIRFilter::new(self.sample_rate.unwrap(), self.fft_width, self.impulse_size, response_shape, self.window_type)
    }

    pub fn sample_rate(mut self, rate: f32) -> FIRFilterBuilder {
        self.sample_rate = Some(rate);
        self
    }

    pub fn window_type(mut self, window_type: WindowType) -> FIRFilterBuilder {
        self.window_type = window_type;
        self
    }

    pub fn fft_width(mut self, fft_width: usize) -> FIRFilterBuilder {
        self.fft_width = fft_width;
        self
    }

    pub fn impulse_size(mut self, impulse_size: usize) -> FIRFilterBuilder {
        self.impulse_size = impulse_size;
        self
    }

    pub fn response_type(mut self, response_type: FIRFilterResponseShape) -> FIRFilterBuilder {
        self.response_type = Some(response_type);
        self
    }
}

impl Default for FIRFilterBuilder {
    fn default() -> Self {
        Self::new()
    }
}
