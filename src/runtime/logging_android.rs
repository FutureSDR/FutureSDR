use once_cell::sync::OnceCell;
use tracing_android::layer;
use tracing_subscriber::prelude::*;

// Make sure tracing is only initialized once.
static TRACING_INIT: OnceCell<()> = OnceCell::new();

pub fn init() {
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
