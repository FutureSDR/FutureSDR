/// This module exists to try to inhibit accidental import of [`futuresdr::anyhow::Result`]
#[cfg(test)]
mod isolated_scope {
    /// Regression test for https://github.com/FutureSDR/FutureSDR/issues/149
    #[test]
    fn message_handler_compiles() {
        use futuresdr::runtime::BlockMeta;
        use futuresdr::runtime::BlockT;
        use futuresdr::runtime::MessageOutputs;
        use futuresdr::runtime::Pmt;
        use futuresdr::runtime::Result;
        use futuresdr::runtime::StreamIoBuilder;
        use futuresdr::runtime::TypedBlock;
        use futuresdr::runtime::WorkIo;

        #[derive(futuresdr::Block)]
        #[message_inputs(r#in)]
        #[null_kernel]
        struct MsgThing;

        impl MsgThing {
            #[allow(clippy::new_ret_no_self)]
            fn new() -> TypedBlock<Self> {
                TypedBlock::new(StreamIoBuilder::new().build(), Self)
            }

            async fn r#in(
                &mut self,
                _io: &mut WorkIo,
                _mio: &mut MessageOutputs,
                _meta: &mut BlockMeta,
                _p: Pmt,
            ) -> Result<Pmt> {
                Ok(Pmt::Ok)
            }
        }

        // Main test is that the above compiles without futuresdr::anyhow::Result being imported in scope.
        let b = MsgThing::new();
        assert!(b.message_input_name_to_id("in").is_some());
    }
}
