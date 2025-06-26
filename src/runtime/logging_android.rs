use tracing_android::layer;
use tracing_subscriber::prelude::*;

pub fn init() {
    let android_layer = layer("FutureSDR").expect("failed to initialize Android tracing layer");
    tracing_subscriber::registry().with(android_layer).init();
}
