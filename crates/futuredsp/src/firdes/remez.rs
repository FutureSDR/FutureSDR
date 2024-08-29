// Routines for designing optimal FIR filters.
//
// For a great intro to how all this stuff works, see section 6.6 of
// "Digital Signal Processing: A Practical Approach", Emmanuael C. Ifeachor
// and Barrie W. Jervis, Adison-Wesley, 1993.  ISBN 0-201-54413-X.

use alloc::vec::Vec;

use super::remez_impl::pm_remez;

/// Builds a low pass filter.
///
/// Args:
///     gain: Filter gain in the passband (linear)
///     Fs: Sampling rate (sps) == number of channels in case of polyphase channelizer
///     freq1: End of pass band (in Hz) == 0.5 - (transition_bw/channel_spacing)/2. in case of polyphase channelizer
///     freq2: Start of stop band (in Hz) == 0.5 + (transition_bw/channel_spacing)/2. in case of polyphase channelizer
///     passband_ripple_db: Pass band ripple in dB (should be small, < 1)
///     stopband_atten_db: Stop band attenuation in dB (should be large, >= 60)
///     nextra_taps: Extra taps to use in the filter (default=2)
pub fn low_pass(
    gain: f64,
    fs: usize,
    freq1: f64,
    freq2: f64,
    passband_ripple_db: f64,
    stopband_atten_db: f64,
    nextra_taps: Option<usize>,
) -> Vec<f64> {
    let nextra_taps = nextra_taps.unwrap_or(2);
    let passband_dev = passband_ripple_to_dev(passband_ripple_db);
    let stopband_dev = stopband_atten_to_dev(stopband_atten_db);
    let (n, fo, ao, w) = remezord(
        &[freq1, freq2],
        &[gain, 0.0_f64],
        &[passband_dev, stopband_dev],
        Some(fs),
    );
    // # The remezord typically under - estimates the filter order, so add 2 taps by default
    pm_remez(n + nextra_taps, &fo, &ao, &w, "bandpass", None)
}

fn stopband_atten_to_dev(atten_db: f64) -> f64 {
    // ""
    // "Convert a stopband attenuation in dB to an absolute value"
    // ""
    10.0_f64.powf(-atten_db / 20.)
}

fn passband_ripple_to_dev(ripple_db: f64) -> f64 {
    // ""
    // "Convert passband ripple spec expressed in dB to an absolute value"
    // ""
    (10.0_f64.powf(ripple_db / 20.) - 1.) / (10.0_f64.powf(ripple_db / 20.) + 1.)
}

//  ----------------------------------------------------------------

fn remezord(
    fcuts: &[f64],
    mags: &[f64],
    devs: &[f64],
    fsamp: Option<usize>,
) -> (usize, Vec<f64>, Vec<f64>, Vec<f64>) {
    // '''
    // FIR order estimator (lowpass, highpass, bandpass, mulitiband).
    //
    // (n, fo, ao, w) = remezord (f, a, dev)
    // (n, fo, ao, w) = remezord (f, a, dev, fs)
    //
    // (n, fo, ao, w) = remezord (f, a, dev) finds the approximate order,
    // normalized frequency band edges, frequency band amplitudes, and
    // weights that meet input specifications f, a, and dev, to use with
    // the remez command.
    //
    // * f is a sequence of frequency band edges (between 0 and Fs/2, where
    //   Fs is the sampling frequency), and a is a sequence specifying the
    //   desired amplitude on the bands defined by f. The length of f is
    //   twice the length of a, minus 2. The desired function is
    //   piecewise constant.
    //
    // * dev is a sequence the same size as a that specifies the maximum
    //   allowable deviation or ripples between the frequency response
    //   and the desired amplitude of the output filter, for each band.
    //
    // Use remez with the resulting order n, frequency sequence fo,
    // amplitude response sequence ao, and weights w to design the filter b
    // which approximately meets the specifications given by remezord
    // input parameters f, a, and dev:
    //
    // b = remez (n, fo, ao, w)
    //
    // (n, fo, ao, w) = remezord (f, a, dev, Fs) specifies a sampling frequency Fs.
    //
    // Fs defaults to 2 Hz, implying a Nyquist frequency of 1 Hz. You can
    // therefore specify band edges scaled to a particular applications
    // sampling frequency.
    //
    // In some cases remezord underestimates the order n. If the filter
    // does not meet the specifications, try a higher order such as n+1
    // or n+2.
    // '''
    // get local copies
    let fsamp = fsamp.unwrap_or(2);
    // fcuts = fcuts[:]
    // mags = mags[:]
    // devs = devs[:]

    let fcuts: Vec<f64> = fcuts.iter().map(|&x| x / fsamp as f64).collect();

    let nf = fcuts.len();
    let nm = mags.len();
    let nd = devs.len();
    let nbands = nm;

    assert!(nm == nd, "Length of mags and devs must be equal");

    assert!(
        nf == 2 * (nbands - 1),
        "Length of f must be 2 * len (mags) - 2"
    );

    let devs: Vec<f64> = devs
        .iter()
        .zip(mags.iter())
        .map(|(&d, &m)| if m == 0. { d } else { d / m })
        .collect(); // if not stopband, get relative deviation

    // separate the passband and stopband edges
    let f1: Vec<f64> = fcuts.iter().step_by(2).copied().collect();
    let f2: Vec<f64> = fcuts[1..].iter().step_by(2).copied().collect();

    let mut n = 0;
    let mut min_delta: f64 = 2.;
    for i in 0..f1.len() {
        if f2[i] - f1[i] < min_delta {
            n = i;
            min_delta = f2[i] - f1[i];
        }
    }
    let l = if nbands == 2 {
        // lowpass or highpass case (use formula)
        lporder(f1[n], f2[n], devs[0], devs[1])
    } else {
        // bandpass or multipass case
        // try different lowpasses and take the worst one that
        //  goes through the BP specs
        let mut l_tmp: f64 = 0.;
        for i in 1..(nbands - 1) {
            let l1 = lporder(f1[i - 1], f2[i - 1], devs[i], devs[i - 1]);
            let l2 = lporder(f1[i], f2[i], devs[i], devs[i + 1]);
            l_tmp = l_tmp.max(l1.max(l2));
        }
        l_tmp
    };

    let n = l.ceil() as usize - 1; // need order, not length for remez

    // cook up remez compatible result
    let mut ff: Vec<f64> = fcuts.iter().copied().map(|x| 2. * x).collect();
    ff.push(1.);
    ff.insert(0, 0.);

    let aa = mags
        .iter()
        .zip(mags.iter())
        .fold(vec![], |mut vec, (&a_1, &a_2)| {
            vec.push(a_1);
            vec.push(a_2);
            vec
        });

    let max_dev = devs.iter().max_by(|a, b| a.total_cmp(b)).unwrap();
    let wts: Vec<f64> = devs.iter().map(|&x| max_dev / x).collect();

    (n, ff, aa, wts)
}

//  ----------------------------------------------------------------

fn lporder(freq1: f64, freq2: f64, delta_p: f64, delta_s: f64) -> f64 {
    // '''
    // FIR lowpass filter length estimator.  freq1 and freq2 are
    // normalized to the sampling frequency.  delta_p is the passband
    // deviation (ripple), delta_s is the stopband deviation (ripple).
    //
    // Note, this works for high pass filters too (freq1 > freq2), but
    // doesn't work well if the transition is near f == 0 or f == fs/2
    //
    // From Herrmann et al (1973), Practical design rules for optimum
    // finite impulse response filters.  Bell System Technical J., 52, 769-99
    // '''
    let df = (freq2 - freq1).abs();
    let ddp = delta_p.log10();
    let dds = delta_s.log10();

    let a1 = 5.309e-3;
    let a2 = 7.114e-2;
    let a3 = -4.761e-1;
    let a4 = -2.66e-3;
    let a5 = -5.941e-1;
    let a6 = -4.278e-1;

    let b1 = 11.01217;
    let b2 = 0.5124401;

    let t1 = a1 * ddp * ddp;
    let t2 = a2 * ddp;
    let t3 = a4 * ddp * ddp;
    let t4 = a5 * ddp;

    let dinf = ((t1 + t2 + a3) * dds) + (t3 + t4 + a6);
    let ff = b1 + b2 * (ddp - dds);

    dinf / df - ff * df + 1.
}
