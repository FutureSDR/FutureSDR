// Copyright 2004,2007,2010,2012-2013 Free Software Foundation, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

use core::iter::Sum;
use core::ops::Mul;

use num_traits::Num;

use crate::ComputationStatus;
use crate::StatefulFilter;

use super::fir_interpolator::FirInterpolator;

/// MMSE Resampler
pub struct Resampler<T>
where
    T: 'static,
{
    d_mu: f32,
    d_mu_inc: f32,
    d_resamp: FirInterpolator<T>,
}

impl<T> Resampler<T>
where
    T: Copy + Num + Sum<T> + Mul<f32, Output = T> + 'static,
{
    /// Create MMSE Resampler.
    pub fn new(resamp_ratio: f32) -> Self {
        Self {
            d_mu: 0.0,
            d_mu_inc: 1.0 / resamp_ratio,
            d_resamp: FirInterpolator::<T>::new(),
        }
    }
}

impl<T> StatefulFilter<T, T, f32> for Resampler<T>
where
    T: Copy + Num + Sum<T> + Mul<f32, Output = T> + 'static,
{
    fn filter(&mut self, i: &[T], o: &mut [T]) -> (usize, usize, ComputationStatus) {
        let ninput_items = i.len().saturating_sub(FirInterpolator::<T>::lookahead());
        let noutput_items = o.len();
        let mut ii: usize = 0;
        let mut oo: usize = 0;

        while ii < ninput_items && oo < noutput_items {
            o[oo] = self.d_resamp.interpolate(
                &i[ii..(ii + FirInterpolator::<T>::lookahead() + 1)],
                self.d_mu,
            );
            oo += 1;

            let s = self.d_mu + self.d_mu_inc;
            let f = s.floor();
            let incr = f as usize;
            self.d_mu = s - f;
            ii += incr;
        }
        let max_in = ii >= ninput_items;
        let max_out = oo >= noutput_items;
        let status = match (max_in, max_out) {
            (true, true) => ComputationStatus::BothSufficient,
            (false, true) => ComputationStatus::InsufficientOutput,
            (true, false) => ComputationStatus::InsufficientInput,
            _ => panic!("MMSE Resampler terminated without fully consuming input or output"),
        };

        (ii, oo, status)
    }
}
