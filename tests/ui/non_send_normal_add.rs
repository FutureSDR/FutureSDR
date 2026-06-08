use std::rc::Rc;

use futuresdr::runtime::dev::prelude::*;

#[derive(Block)]
#[null_kernel]
struct NonSendBlock {
    _state: Rc<()>,
}

fn main() {
    let mut fg = Flowgraph::new();
    let _ = fg.add(NonSendBlock { _state: Rc::new(()) });
}
