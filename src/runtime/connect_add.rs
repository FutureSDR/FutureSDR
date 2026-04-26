use crate::runtime::BlockRef;
use crate::runtime::Error;
use crate::runtime::Flowgraph;
use crate::runtime::Result;
use crate::runtime::dev::Kernel;
use crate::runtime::kernel_interface::KernelInterface;

#[doc(hidden)]
pub trait ConnectAdd {
    type Added;

    fn connect_add(self, fg: &mut Flowgraph) -> Result<Self::Added, Error>;
}

impl<K> ConnectAdd for K
where
    K: Kernel + KernelInterface + 'static,
{
    type Added = BlockRef<K>;

    fn connect_add(self, fg: &mut Flowgraph) -> Result<Self::Added, Error> {
        Ok(fg.add(self))
    }
}

impl<K: Kernel + 'static> ConnectAdd for BlockRef<K> {
    type Added = BlockRef<K>;

    fn connect_add(self, fg: &mut Flowgraph) -> Result<Self::Added, Error> {
        self.with(&*fg, |_| ())?;
        Ok(self)
    }
}
