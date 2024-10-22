/// This module exists to try to inhibit accidental import of [`futuresdr::anyhow::Result`]
#[cfg(test)]
mod isolated_scope {
    /// Regression test for https://github.com/FutureSDR/FutureSDR/issues/149
    #[test]
    fn message_handler_compiles() {
        use futuresdr::macros::message_handler;
        use futuresdr::runtime::{
            Block, BlockMeta, BlockMetaBuilder, Kernel, MessageIo, MessageIoBuilder, Pmt,
            StreamIoBuilder, WorkIo,
        };

        struct MsgThing;

        impl MsgThing {
            fn new() -> Block {
                Block::new(
                    BlockMetaBuilder::new("MsgThing").build(),
                    StreamIoBuilder::new().build(),
                    MessageIoBuilder::new()
                        .add_input("in", Self::in_port)
                        .build(),
                    Self,
                )
            }

            #[message_handler]
            async fn in_port(
                &mut self,
                _io: &mut WorkIo,
                _mio: &mut MessageIo<Self>,
                _meta: &mut BlockMeta,
                _p: Pmt,
            ) -> Result<Pmt> {
                Ok(Pmt::Ok)
            }
        }

        impl Kernel for MsgThing {}

        // Main test is that the above compiles without futuresdr::anyhow::Result being imported in scope.
        let b = MsgThing::new();
        assert!(b.message_input_name_to_id("in").is_some());
    }
}
