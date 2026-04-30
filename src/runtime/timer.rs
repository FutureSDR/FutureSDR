use std::time::Duration;

/// Cross-target timer used by FutureSDR async code.
pub struct Timer;

impl Timer {
    /// Complete after `duration` has elapsed.
    pub async fn after(duration: Duration) {
        #[cfg(not(target_arch = "wasm32"))]
        async_io::Timer::after(duration).await;
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::sleep(duration).await;
    }
}
