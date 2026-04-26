use std::future::Future;

use crate::runtime::dev::BlockMeta;
use crate::runtime::dev::MaybeSend;
use crate::runtime::dev::MessageOutputs;
use crate::runtime::dev::WorkIo;
use futuresdr::runtime::Result;

/// Kernel
///
/// Central trait that the developer has to implement for a block.
pub trait Kernel: MaybeSend {
    /// Processes stream data
    fn work(
        &mut self,
        _io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> + MaybeSend {
        async { Ok(()) }
    }
    /// Initialize kernel
    fn init(
        &mut self,
        _mo: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> + MaybeSend {
        async { Ok(()) }
    }
    /// De-initialize kernel
    fn deinit(
        &mut self,
        _mo: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> impl Future<Output = Result<()>> + MaybeSend {
        async { Ok(()) }
    }
}
