mod fft_shift;
use fft_shift::FftShift;
mod keep_1_in_n;
use keep_1_in_n::Keep1InN;

use std::env;

use futuresdr::anyhow::Result;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Fft;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::log::info;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

pub fn run_fg() -> Result<()> {
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let mut args = Vec::new();
    if let Ok(s) = env::var("FUTURESDR_usbfs_dir") {
        args.push(format!("usbfs={}", s));
    }

    if let Ok(s) = env::var("FUTURESDR_usb_fd") {
        args.push(format!("fd={}", s));
    }

    let args = args.join(",");
    info!("soapy device filter {}", &args);

    let src = SoapySourceBuilder::new()
        .filter(args)
        .freq(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build();
    let snk = WebsocketSinkBuilder::<f32>::new(9001)
        .mode(WebsocketSinkMode::FixedDropping(2048))
        .build();

    let src = fg.add_block(src);
    let fft = fg.add_block(Fft::new(2048));
    let power = fg.add_block(Apply::new(|x: &Complex32| x.norm()));
    let log = fg.add_block(Apply::new(|x: &f32| 10.0 * x.log10()));
    let shift = fg.add_block(FftShift::<f32>::new());
    let keep = fg.add_block(Keep1InN::new(0.1, 10));
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", fft, "in")?;
    fg.connect_stream(fft, "out", power, "in")?;
    fg.connect_stream(power, "out", log, "in")?;
    fg.connect_stream(log, "out", shift, "in")?;
    fg.connect_stream(shift, "out", keep, "in")?;
    fg.connect_stream(keep, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}

#[cfg(target_os = "android")]
mod android {
    use super::*;
    use jni::objects::{JClass, JString};
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
