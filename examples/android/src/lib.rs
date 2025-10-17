use anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::MovingAvg;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;

pub fn run_fg(fd: u32) -> Result<()> {
    let mut fg = Flowgraph::new();

    let args = format!("fd={fd}");
    info!("device args {}", &args);

    let src = Builder::new(args)?
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build_source()?;
    let snk = WebsocketSinkBuilder::<f32>::new(9001)
        .mode(WebsocketSinkMode::FixedDropping(2048))
        .build();
    let fft: Fft = Fft::with_options(2048, FftDirection::Forward, true, None);
    let power = Apply::<_, _, _>::new(|x: &Complex32| x.norm());
    let log = Apply::<_, _, _>::new(|x: &f32| 10.0 * x.log10());
    let keep = MovingAvg::<2048>::new(0.1, 10);
    info!("blocks constructed");

    connect!(fg, src.outputs[0] > fft > power > log > keep > snk);
    info!("connected, starting");

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
