use std::fmt;
use std::future::Future;
use std::pin::Pin;

use crate::runtime::dev::MaybeSend;

/// Work IO
///
/// Communicate between `work()` and the runtime.
pub struct WorkIo {
    /// Call block immediately again
    pub call_again: bool,
    /// Mark block as finished
    pub finished: bool,
    /// Block on future
    ///
    /// The block will be called (1) if somehting happens or (2) if the future resolves
    #[cfg(not(target_arch = "wasm32"))]
    pub block_on: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
    /// The block will be called (1) if somehting happens or (2) if the future resolves
    #[cfg(target_arch = "wasm32")]
    pub block_on: Option<Pin<Box<dyn Future<Output = ()>>>>,
}

impl WorkIo {
    /// Helper to set the future of the Work IO
    pub fn block_on<F: Future<Output = ()> + MaybeSend + 'static>(&mut self, f: F) {
        self.block_on = Some(Box::pin(f));
    }
}

impl fmt::Debug for WorkIo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("WorkIo")
            .field("call_again", &self.call_again)
            .field("finished", &self.finished)
            .finish()
    }
}
