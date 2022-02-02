#[cfg(not(RUSTC_IS_STABLE))]
use std::intrinsics::{fadd_fast, fmul_fast};

use std::mem;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::SyncKernel;
use crate::runtime::WorkIo;
use num_complex::Complex;

pub trait FirKernel<SampleType>: Send {
    /// Returns (samples consumed, samples produced)
    fn work(&self, input: &[SampleType], output: &mut [SampleType]) -> (usize, usize);
}

pub trait TapsAccessor: Send {
    type TapType;

    fn num_taps(&self) -> usize;

    /// Gets the `index`th tap.
    ///
    /// Safety: The invariant `index < num_taps()` must be upheld.
    unsafe fn get(&self, index: usize) -> Self::TapType;
}

impl<const N: usize> TapsAccessor for [f32; N] {
    type TapType = f32;

    fn num_taps(&self) -> usize {
        N
    }

    unsafe fn get(&self, index: usize) -> f32 {
        debug_assert!(index < num_taps());
        *self.get_unchecked(index)
    }
}

pub struct FirKernelCore<SampleType, TapsType: TapsAccessor> {
    taps: TapsType,
    _sampletype: std::marker::PhantomData<SampleType>,
}

#[cfg(not(RUSTC_IS_STABLE))]
impl<TapsType: TapsAccessor<TapType = f32>> FirKernel<f32> for FirKernelCore<f32, TapsType> {
    fn work(&self, i: &[f32], o: &mut [f32]) -> (usize, usize) {
        let n = std::cmp::min((i.len() + 1).saturating_sub(self.taps.num_taps()), o.len());

        unsafe {
            for k in 0..n {
                let mut sum = 0.0;
                for t in 0..self.taps.num_taps() {
                    sum = fadd_fast(sum, fmul_fast(*i.get_unchecked(k + t), self.taps.get(t)));
                }
                *o.get_unchecked_mut(k) = sum;
            }
        }

        (n, n)
    }
}

#[cfg(RUSTC_IS_STABLE)]
impl<TapsType: TapsAccessor<TapType = f32>> FirKernel<f32> for FirKernelCore<f32, TapsType> {
    fn work(&self, i: &[f32], o: &mut [f32]) -> (usize, usize) {
        let n = std::cmp::min((i.len() + 1).saturating_sub(self.taps.num_taps()), o.len());

        unsafe {
            for k in 0..n {
                let mut sum = 0.0;
                for t in 0..self.taps.num_taps() {
                    sum += i.get_unchecked(k + t) * self.taps.get(t);
                }
                *o.get_unchecked_mut(k) = sum;
            }
        }

        (n, n)
    }
}

#[cfg(not(RUSTC_IS_STABLE))]
impl<TapsType: TapsAccessor<TapType = f32>> FirKernel<Complex<f32>>
    for FirKernelCore<Complex<f32>, TapsType>
{
    fn work(&self, i: &[Complex<f32>], o: &mut [Complex<f32>]) -> (usize, usize) {
        let n = std::cmp::min((i.len() + 1).saturating_sub(self.taps.num_taps()), o.len());

        unsafe {
            for k in 0..n {
                let mut sum_re = 0.0;
                let mut sum_im = 0.0;
                for t in 0..self.taps.num_taps() {
                    sum_re = fadd_fast(
                        sum_re,
                        fmul_fast(i.get_unchecked(k + t).re, self.taps.get(t)),
                    );
                    sum_im = fadd_fast(
                        sum_im,
                        fmul_fast(i.get_unchecked(k + t).im, self.taps.get(t)),
                    );
                }
                *o.get_unchecked_mut(k) = Complex {
                    re: sum_re,
                    im: sum_im,
                };
            }
        }

        (n, n)
    }
}

#[cfg(RUSTC_IS_STABLE)]
impl<TapsType: TapsAccessor<TapType = f32>> FirKernel<Complex<f32>>
    for FirKernelCore<Complex<f32>, TapsType>
{
    fn work(&self, i: &[Complex<f32>], o: &mut [Complex<f32>]) -> (usize, usize) {
        let n = std::cmp::min((i.len() + 1).saturating_sub(self.taps.num_taps()), o.len());

        unsafe {
            for k in 0..n {
                let mut sum_re = 0.0;
                let mut sum_im = 0.0;
                for t in 0..self.taps.num_taps() {
                    sum_re += i.get_unchecked(k + t).re * self.taps.get(t);
                    sum_im += i.get_unchecked(k + t).im * self.taps.get(t);
                }
                *o.get_unchecked_mut(k) = Complex {
                    re: sum_re,
                    im: sum_im,
                };
            }
        }

        (n, n)
    }
}

pub struct Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + FirKernel<SampleType>,
{
    core: Core,
    _sampletype: std::marker::PhantomData<SampleType>,
    _taptype: std::marker::PhantomData<TapType>,
}

unsafe impl<SampleType, TapType, Core> Send for Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + FirKernel<SampleType>,
{
}

impl<SampleType, TapType, Core> Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + FirKernel<SampleType>,
{
    pub fn new(core: Core) -> Block {
        Block::new_sync(
            BlockMetaBuilder::new("Fir").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<SampleType>())
                .add_output("out", mem::size_of::<SampleType>())
                .build(),
            MessageIoBuilder::<Fir<SampleType, TapType, Core>>::new().build(),
            Fir {
                core: core,
                _sampletype: std::marker::PhantomData,
                _taptype: std::marker::PhantomData,
            },
        )
    }
}

#[async_trait]
impl<SampleType, TapType, Core> SyncKernel for Fir<SampleType, TapType, Core>
where
    SampleType: 'static + Send,
    TapType: 'static,
    Core: 'static + FirKernel<SampleType>,
{
    fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<SampleType>();
        let o = sio.output(0).slice::<SampleType>();

        let (consumed, produced) = self.core.work(i, o);

        sio.input(0).consume(consumed);
        sio.output(0).produce(produced);

        if consumed == 0 && sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}

pub struct FirBuilder {
    //
}

impl FirBuilder {
    /// Constructs a new FIR Filter using the given types and taps. This function will
    /// pick the optimal FIR implementation for the given constraints.
    ///
    /// Note that there must be an implementation of `TapsAccessor` for the taps object
    /// you pass in. Implementations are provided for arrays.
    ///
    /// Additionally, there must be an available core (implementation of `FirKernel`) for
    /// the specified `SampleType` and `TapType`. Cores are provided for the following
    /// `SampleType`/`TapType` combinations:
    /// - `SampleType=f32`, `TapType=f32`
    /// - `SampleType=Complex<f32>`, `TapType=f32`
    ///
    /// Example usage:
    /// ```
    /// use futuresdr::blocks::FirBuilder;
    ///
    /// let fir = FirBuilder::new::<f32, f32, _>([1.0, 2.0, 3.0]);
    /// ```
    pub fn new<SampleType, TapType, Taps>(taps: Taps) -> Block
    where
        SampleType: 'static + Send,
        TapType: 'static,
        Taps: 'static + TapsAccessor,
        FirKernelCore<SampleType, Taps>: FirKernel<SampleType>,
    {
        Fir::<SampleType, TapType, FirKernelCore<SampleType, Taps>>::new(FirKernelCore {
            taps: taps,
            _sampletype: std::marker::PhantomData,
        })
    }
}
