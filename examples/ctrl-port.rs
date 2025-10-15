use anyhow::Result;
use futuresdr::prelude::*;

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    fg.add_block(CtrlPortDemo::new());

    info!("Ways to interact with the flowgraph:");
    info!("Web GUI: http://127.0.0.1:1337");
    info!("Flowgraph JSON: curl http://127.0.0.1:1337/api/fg/0/");
    info!("Block JSON: curl http://127.0.0.1:1337/api/fg/0/block/0/");
    info!("Block Callback (GET): curl http://127.0.0.1:1337/api/fg/0/block/0/call/myhandler/");
    info!(
        r#"Block Callback (POST): curl -X POST -H "Content-Type: application/json" -d '{{ "U32": 123 }}'  http://127.0.0.1:1337/api/fg/0/block/0/call/myhandler/"#
    );

    Runtime::new().run(fg)?;
    Ok(())
}

#[derive(Block)]
#[message_inputs(myhandler)]
#[null_kernel]
pub struct CtrlPortDemo {
    counter: u64,
}

impl CtrlPortDemo {
    pub fn new() -> Self {
        Self { counter: 5 }
    }

    async fn myhandler(
        &mut self,
        _io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        println!("pmt {:?}, counter {}", p, self.counter);
        self.counter += 1;
        Ok(Pmt::U64(self.counter - 1))
    }
}

impl Default for CtrlPortDemo {
    fn default() -> Self {
        Self::new()
    }
}
