use crate::BATCH_SIZE;
use crate::FFT_SIZE;
use burn::prelude::*;
use std::f32::consts::PI;

pub fn bit_reversal_indices(log_n: usize) -> Vec<usize> {
    let n = 1 << log_n;
    let mut rev = vec![0; n];
    for (i, r) in rev.iter_mut().enumerate() {
        *r = (0..log_n).fold(0, |acc, b| acc << 1 | ((i >> b) & 1));
    }
    rev
}

pub fn mul_complex4<B: Backend>(
    a: Tensor<B, 4, Float>, // [batch, groups, half, 2]
    b: Tensor<B, 4, Float>, // [batch, groups, half, 2]
) -> Tensor<B, 4, Float> {
    // split real/imag from a and b
    let a_re = a.clone().slice(s![.., .., .., 0]);
    let a_im = a.clone().slice(s![.., .., .., 1]);
    let b_re = b.clone().slice(s![.., .., .., 0]);
    let b_im = b.clone().slice(s![.., .., .., 1]);

    // (ar·br − ai·bi), (ar·bi + ai·br)
    let real = a_re
        .clone()
        .mul(b_re.clone())
        .sub(a_im.clone().mul(b_im.clone()));
    let imag = a_re
        .clone()
        .mul(b_im.clone())
        .add(a_im.clone().mul(b_re.clone()));

    // concat into [batch, groups, half, 2]
    Tensor::cat(vec![real, imag], 3)
}

pub fn generate_stage_twiddles<B: Backend>(
    stage: usize,
    device: &Device<B>,
) -> Tensor<B, 2, Float> {
    let m = 1 << stage; // Stage size
    let half = m >> 1; // Number of twiddle factors needed

    // Generate k values [0..half]
    let k = Tensor::<B, 1, Int>::arange(0..half as i64, device);

    // Calculate angles: -2π * k / m
    let angles = k.float().mul_scalar(-2.0 * PI / m as f32);

    // Generate complex exponentials
    let real = angles.clone().cos();
    let imag = angles.sin();

    // Stack into [half, 2] tensor
    Tensor::stack(vec![real, imag], 1)
}

/// In-place radix-2 FFT on a batch of complex vectors
pub fn fft_inplace<B: Backend>(
    input: Tensor<B, 3, Float>, // shape [batch, N, 2]
    rev: Tensor<B, 3, Int>,
    twiddles: &[Tensor<B, 4, Float>],
) -> Tensor<B, 3, Float> {
    let mut x = input.gather(1, rev); // shape [batch, N, 2]

    // 3) Iterative butterfly stages
    for (s, twiddle) in twiddles.iter().enumerate().skip(1) {
        let m = 1 << s;
        let half = m >> 1;
        let groups = FFT_SIZE / m;

        let wm_half = twiddle.clone();
        let wm_tiled = wm_half.repeat_dim(0, BATCH_SIZE).repeat_dim(1, groups);

        let x_blocks = x.clone().reshape([BATCH_SIZE, groups, m, 2]);

        let even = x_blocks.clone().slice(s![.., .., 0..half, ..]);
        let odd = x_blocks.slice(s![.., .., half..m, ..]);
        let odd_t = mul_complex4(odd, wm_tiled);

        let top = even.clone().add(odd_t.clone());
        let bottom = even.sub(odd_t);

        // flatten the two spatial dims:
        x = Tensor::cat(vec![top, bottom], 2).reshape([BATCH_SIZE, FFT_SIZE, 2])
    }
    x
}
