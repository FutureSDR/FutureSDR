use std::env;
use std::mem::size_of;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::log::info;
use futuresdr::blocks::FftBuilder;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::num_complex::Complex;
use futuresdr::runtime::AsyncKernel;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

pub fn run_fg() -> Result<()> {
    let mut fg = Flowgraph::new();

    // let mut args = vec![String::from("driver=RTLSDR")];
    let mut args = Vec::new();
    if let Ok(s) = env::var("FUTURESDR_usbfs_dir") {
        args.push(format!("usbfs={} ", s));
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
    let fft = FftBuilder::new().build();
    let mag = ComplexToMag::new();
    let snk = WebsocketSinkBuilder::<f32>::new(9001)
        .mode(WebsocketSinkMode::FixedDropping(2048))
        .build();

    let src = fg.add_block(src);
    let fft = fg.add_block(fft);
    let mag = fg.add_block(mag);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", fft, "in")?;
    fg.connect_stream(fft, "out", mag, "in")?;
    fg.connect_stream(mag, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}

pub struct ComplexToMag {}

impl ComplexToMag {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new_async(
            BlockMetaBuilder::new("ComplexToMag").build(),
            StreamIoBuilder::new()
                .add_input("in", size_of::<Complex<f32>>())
                .add_output("out", size_of::<f32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {},
        )
    }
}

#[async_trait]
impl AsyncKernel for ComplexToMag {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex<f32>>();
        let o = sio.output(0).slice::<f32>();

        let n = std::cmp::min(i.len(), o.len());

        for x in 0..n {
            let mut t = ((i[x].norm_sqr().log10() + 3.0) / 6.0).mul_add(255.0, 125.0) / 2.0;
            t = t.clamp(0.0, 255.0);
            o[x] = t;
        }

        if sio.input(0).finished() && n == i.len() {
            io.finished = true;
        }

        if n == 0 {
            return Ok(());
        }

        sio.input(0).consume(n);
        sio.output(0).produce(n);

        Ok(())
    }
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
        std::env::set_var("SOAPY_SDR_PLUGIN_PATH", ".:lib:lib/arm64-v8a");
        std::env::set_var("RUST_LOG", "debug");
        run_fg().unwrap();
    }
}
