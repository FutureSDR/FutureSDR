use std::fmt;

/// Work-loop control flags passed to [`Kernel::work`](crate::runtime::dev::Kernel::work).
///
/// A block sets these fields during `work()` to tell the runtime whether it
/// should run the block again immediately or stop the block.
pub struct WorkIo {
    /// Schedule the block again immediately after the current `work()` call.
    ///
    /// Use this when the block knows it can make more progress without waiting
    /// for a runtime notification.
    pub call_again: bool,
    /// Mark the block as finished.
    ///
    /// Once set, the runtime stops calling `work()` for the block and notifies
    /// connected downstream ports.
    pub finished: bool,
}

impl fmt::Debug for WorkIo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("WorkIo")
            .field("call_again", &self.call_again)
            .field("finished", &self.finished)
            .finish()
    }
}
