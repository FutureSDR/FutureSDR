use anyhow::Result;
use futuresdr::blocks::seify::Builder;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::MovingAvg;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::prelude::*;
use std::env;

pub fn run_fg() -> Result<()> {
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let mut args = Vec::new();
    if let Ok(s) = env::var("FUTURESDR_usbfs_dir") {
        args.push(format!("usbfs={s}"));
    }

    if let Ok(s) = env::var("FUTURESDR_usb_fd") {
        args.push(format!("fd={s}"));
    }

    let args = args.join(",");
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

    connect!(fg, src.outputs[0] > fft > power > log > keep > snk);

    Runtime::new().run(fg)?;
    Ok(())
}

#[cfg(target_os = "android")]
mod android {
    use super::*;
    use jni::objects::JClass;
    use jni::objects::JString;
    use jni::sys::jint;
    use jni::JNIEnv;

    #[allow(non_snake_case)]
    #[no_mangle]
    pub extern "system" fn Java_net_bastibl_futuresdrhw_MainActivity_runFg(
        env: JNIEnv,
        _class: JClass,
        fd: jint,
        usbfs_dir: JString,
        tmp_dir: JString,
    ) {
        let tmp_dir: String = env
            .get_string(tmp_dir)
            .expect("Couldn't get java string!")
            .into();
        std::env::set_var("FUTURESDR_tmp_dir", tmp_dir);
        let usbfs_dir: String = env
            .get_string(usbfs_dir)
            .expect("Couldn't get java string!")
            .into();
        std::env::set_var("FUTURESDR_usbfs_dir", usbfs_dir);
        std::env::set_var("FUTURESDR_usb_fd", format!("{}", fd as i32));
        std::env::set_var("FUTURESDR_ctrlport_enable", "true");
        std::env::set_var("FUTURESDR_ctrlport_bind", "0.0.0.0:1337");
        run_fg().unwrap();
    }
}
