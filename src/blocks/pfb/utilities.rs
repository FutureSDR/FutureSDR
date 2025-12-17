use num_complex::Complex32;

use futuredsp::FirFilter;

pub fn partition_filter_taps(
    taps: &[f32],
    n_filters: usize,
) -> (Vec<FirFilter<Complex32, Complex32, Vec<f32>>>, usize) {
    let mut fir_filters = vec![];
    let taps_per_filter = (taps.len() as f32 / n_filters as f32).ceil() as usize;
    for i in 0..n_filters {
        let pad = taps_per_filter - ((taps.len() - i) as f32 / n_filters as f32).ceil() as usize;
        let taps_tmp: Vec<f32> = taps
            .iter()
            .skip(i)
            .step_by(n_filters)
            .copied()
            // .rev()
            .chain(std::iter::repeat_n(0.0, pad))
            .collect();
        debug_assert_eq!(taps_tmp.len(), taps_per_filter);
        fir_filters.push(FirFilter::<Complex32, Complex32, _>::new(taps_tmp));
    }
    (fir_filters, taps_per_filter)
}
