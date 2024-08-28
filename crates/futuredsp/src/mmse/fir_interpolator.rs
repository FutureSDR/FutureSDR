// Copyright 2002,2012 Free Software Foundation, Inc.
// SPDX-License-Identifier: GPL-3.0-or-later

use core::iter::Sum;
use core::marker::PhantomData;
use core::ops::Mul;

use num_traits::Num;

use super::taps::*;

fn build_filters() -> [[f32; NTAPS]; NSTEPS + 1] {
    let mut filters = [[0.0; NTAPS]; NSTEPS + 1];
    for f in 0..NSTEPS + 1 {
        for t in 0..NTAPS {
            filters[f][t] = TAPS[f][t] as f32;
        }
    }
    filters
}

/// Compute intermediate samples between signal samples x(k//Ts)
///
/// This implements a Minimum Mean Squared Error interpolator with
/// 8 taps. It is suitable for signals where the bandwidth of
/// interest B = 1/(4//Ts) Where Ts is the time between samples.
///
/// Although mu, the fractional delay, is specified as a float, it
/// is actually quantized. 0.0 <= mu <= 1.0. That is, mu is
/// quantized in the interpolate method to 32nd's of a sample.
pub(super) struct FirInterpolator<T> {
    filters: [[f32; NTAPS]; NSTEPS + 1],
    _p: PhantomData<T>,
}

impl<T> FirInterpolator<T>
where
    T: Copy + Num + Sum<T> + Mul<f32, Output = T> + 'static,
{
    pub fn new() -> Self {
        Self {
            filters: build_filters(),
            _p: PhantomData,
        }
    }

    /// Compute a single interpolated output value.
    ///
    /// The input must have NTAPS valid entries.
    /// input[0] .. input[NTAPS - 1] are referenced to compute the output value.
    ///
    /// `mu` must be in the range [0, 1] and specifies the fractional delay.
    ///
    /// Returns the interpolated input value.
    pub fn interpolate(&self, input: &[T], mu: f32) -> T {
        let imu: usize = (mu * NSTEPS as f32).round() as usize;

        debug_assert!(
            (0.0..=1.0).contains(&mu),
            "MMSE FIR Interpolator: mu out of bounds."
        );
        debug_assert!(
            imu <= NSTEPS,
            "MMSE FIR Interpolator: imu out of bounds ({imu})."
        );

        input[..NTAPS]
            .iter()
            .zip(self.filters[imu].iter())
            .map(|(&x, &y)| x * y)
            .sum()
    }
    /// Number of future input samples required to compute an output sample.
    pub const fn lookahead() -> usize {
        NTAPS - 1
    }
}

impl<T> Default for FirInterpolator<T>
where
    T: Copy + Num + Sum<T> + Mul<f32, Output = T> + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}
