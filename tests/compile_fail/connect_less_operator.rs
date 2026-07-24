use futuresdr::prelude::*;

fn main() {
    let mut fg = Flowgraph::new();
    connect!(fg, src < snk);
}
