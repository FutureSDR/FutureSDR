use futuresdr_remote::Error;
use futuresdr_remote::Remote;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let remote = Remote::new("http://127.0.0.1:1337");

    let mut fgs = remote.flowgraphs().await?;
    let fg = &mut fgs[0];
    fg.update().await?;
    println!("flowgraph {}", &fg);

    let mut blocks = fg.blocks();
    let b = &mut blocks[0];
    b.update().await?;
    println!("block {}", &b);

    println!("Connections:");
    let msg_connections = fg.message_connections();
    for c in msg_connections {
        println!("{}", c);
    }
    let stream_connections = fg.stream_connections();
    for c in stream_connections {
        println!("{}", c);
    }

    Ok(())
}
