use anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::XlatingFir;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::seify::Builder;
use futuresdr::futuredsp::firdes;
use futuresdr::prelude::*;

pub fn run_fg(fd: u32) -> Result<()> {
    let mut fg = Flowgraph::new();

    let args = format!("fd={fd}");
    info!("device args {}", &args);

    let src = Builder::new(args)?
        .frequency(105.3e6 - 0.3e6)
        .sample_rate(3.2e6)
        .gain(40.0)
        .build_source()?;

    let xlate: XlatingFir = XlatingFir::new(10, 0.3e6, 3.2e6);

    let mut last = Complex32::new(1.0, 0.0);
    let demod = Apply::<_, _, _>::new(move |v: &Complex32| -> f32 {
        let arg = (v * last.conj()).arg();
        last = *v;
        arg / 8.0
    });

    let cutoff = 4000.0 / 3.2e5;
    let transition = 2000.0 / 3.2e5;
    let audio_filter_taps = firdes::kaiser::lowpass::<f32>(cutoff, transition, 0.1);
    let resamp2 = FirBuilder::resampling_with_taps::<f32, f32, _>(1, 10, audio_filter_taps);
    let snk = AudioSink::new(32000, 1);

    connect!(fg, src.outputs[0] > xlate > demod > resamp2 > snk);

    Runtime::new().run(fg)?;
    Ok(())
}

#[cfg(target_os = "android")]
mod android {
    use super::*;
    use jni::JNIEnv;
    use jni::objects::JClass;
    use jni::sys::jint;

    #[allow(non_snake_case)]
    #[unsafe(no_mangle)]
    pub extern "system" fn Java_net_bastibl_futuresdrhw_MainActivity_runFg(
        _env: JNIEnv,
        _class: JClass,
        fd: jint,
    ) {
        futuresdr::runtime::init();
        unsafe {
            std::env::set_var("FUTURESDR_ctrlport_enable", "true");
            std::env::set_var("FUTURESDR_ctrlport_bind", "0.0.0.0:1337");
        }

        info!("calling run_fg");
        let ret = run_fg(fd as u32);
        info!("run_fg returned {:?}", ret);
    }
}
