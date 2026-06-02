pub fn init() {
    #[cfg(all(target_os = "android", not(target_arch = "wasm32")))]
    android::init();
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    native::init();
    #[cfg(target_arch = "wasm32")]
    wasm::init();
}

#[cfg(all(target_os = "android", not(target_arch = "wasm32")))]
mod android {
    use once_cell::sync::OnceCell;
    use tracing_android::layer;
    use tracing_subscriber::prelude::*;

    // Make sure tracing is only initialized once.
    static TRACING_INIT: OnceCell<()> = OnceCell::new();

    pub(super) fn init() {
        TRACING_INIT.get_or_init(|| match layer("FutureSDR") {
            Ok(android_layer) => {
                let subscriber = tracing_subscriber::registry().with(android_layer);
                if let Err(e) = subscriber.try_init() {
                    eprintln!("tracing already initialized or failed: {e}");
                }
            }
            Err(e) => {
                eprintln!("failed to initialize Android tracing layer: {e}");
            }
        });
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    pub(super) fn init() {
        let _ = tracing_wasm::try_set_as_global_default();
    }
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
mod native {
    use tracing_subscriber::filter::EnvFilter;
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;

    use crate::runtime::config;

    pub(super) fn init() {
        let format = fmt::layer()
            .with_level(true)
            .with_target(true)
            .with_thread_ids(false)
            .with_thread_names(true)
            .compact();

        let level = config::config().log_level;
        let filter = EnvFilter::builder()
            .with_default_directive(level.into())
            .with_env_var("FUTURESDR_LOG")
            .from_env_lossy();

        let subscriber = tracing_subscriber::registry().with(filter).with(format);

        if tracing::subscriber::set_global_default(subscriber).is_err() {
            tracing::debug!("logger already initialized");
        }
    }
}
