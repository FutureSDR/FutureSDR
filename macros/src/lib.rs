use proc_macro2::Ident;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use proc_macro2::TokenTree;
use quote::quote;
use quote::quote_spanned;
use std::collections::HashSet;
use std::iter::Peekable;

//=========================================================================
// CONNECT
//=========================================================================

/// Avoid boilerplate when setting up the flowgraph.
///
/// This macro simplifies adding blocks to the flowgraph and connecting them.
/// Assume you have created a flowgraph `fg` and several blocks (`src`, `shift`,
/// ...) and need to add the block to the flowgraph and connect them. Using the
/// `connect!` macro, this can be done with:
///
/// ```ignore
/// connect!(fg,
///     src.out > shift.in;
///     shift > resamp1 > demod;
///     demod > resamp2 > snk;
/// );
/// ```
///
/// It generates the following code:
///
/// ```ignore
/// // Add all the blocks to the `Flowgraph`...
/// let src = fg.add_block(src);
/// let shift = fg.add_block(shift);
/// let resamp1 = fg.add_block(resamp1);
/// let demod = fg.add_block(demod);
/// let resamp2 = fg.add_block(resamp2);
/// let snk = fg.add_block(snk);
///
/// // ... and connect the ports appropriately
/// fg.connect_stream(src, "out", shift, "in")?;
/// fg.connect_stream(shift, "out", resamp1, "in")?;
/// fg.connect_stream(resamp1, "out", demod, "in")?;
/// fg.connect_stream(demod, "out", resamp2, "in")?;
/// fg.connect_stream(resamp2, "out", snk, "in")?;
/// ```
///
/// Connections endpoints are defined by `block.port_name`. Standard names
/// (i.e., `out`/`in`) can be omitted. When ports have different name than
/// standard `in` and `out`, one can use following notation.
///
/// Stream connections are indicated as `>`, while message connections are
/// indicated as `|`.
///
/// It is possible to add blocks that have no connections by just putting them
/// on a line separately.
///
/// ```ignore
/// connect!(fg, dummy);
/// ```
///
/// Port names with spaces have to be quoted.
///
/// ```ignore
/// connect!(fg,
///     src."out port" > snk
/// );
/// ```
#[proc_macro]
pub fn connect(attr: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // println!("{}", attr.clone());
    // for a in attr.clone().into_iter() {
    //     println!("{:?}", a);
    // }
    let mut attrs = TokenStream::from(attr).into_iter().peekable();
    let mut out = TokenStream::new();

    let mut blocks = HashSet::<Ident>::new();
    let mut message_connections = HashSet::<(Ident, String, Ident, String)>::new();
    let mut stream_connections = HashSet::<(Ident, String, Ident, String)>::new();

    // search flowgraph variable
    let n = attrs.next();
    let fg = if let Some(TokenTree::Ident(fg)) = n {
        fg
    } else if n.is_none() {
        return quote! {
            compile_error!("Connect macro expects flowgraph and connections as arguments.")
        }
        .into();
    } else {
        return quote_spanned!{
            n.unwrap().span() => compile_error!("Connect macro expects flowgraph as first argument.")
        }.into();
    };

    // search separator
    let n = attrs.next();
    if n.is_none() || !matches!(n.as_ref().unwrap(), &TokenTree::Punct(_)) {
        return quote_spanned! {
            n.unwrap().span() => compile_error!("Connect macro expects separator after flowgraph")
        }
        .into();
    }

    // search for connections
    loop {
        let res = parse_connections(&mut attrs);
        match res {
            ParseResult::Connections {
                stream,
                message,
                blocks: b,
            } => {
                for c in stream.into_iter() {
                    blocks.insert(c.0.clone());
                    blocks.insert(c.2.clone());
                    stream_connections.insert(c);
                }
                for c in message.into_iter() {
                    blocks.insert(c.0.clone());
                    blocks.insert(c.2.clone());
                    message_connections.insert(c);
                }
                for block in b.into_iter() {
                    blocks.insert(block);
                }
            }
            ParseResult::Done => break,
            ParseResult::Error(span, string) => {
                if let Some(span) = span {
                    return quote_spanned! {
                        span => compile_error!(#string)
                    }
                    .into();
                } else {
                    return quote! {
                        compile_error!(#string)
                    }
                    .into();
                }
            }
        }
    }
    // Add the blocks to the flowgraph
    for blk_id in blocks {
        out.extend(quote! {
            #[allow(unused_variables)]
            let #blk_id = #fg.add_block(#blk_id);
        });
    }
    // Stream connections
    for (src, src_port, dst, dst_port) in stream_connections.into_iter() {
        out.extend(quote! {
            #fg.connect_stream(#src, #src_port, #dst, #dst_port)?;
        });
    }
    // Message connections
    for (src, src_port, dst, dst_port) in message_connections.into_iter() {
        out.extend(quote! {
            #fg.connect_message(#src, #src_port, #dst, #dst_port)?;
        });
    }

    // println!("code {}", out);
    out.into()
}

enum ParseResult {
    Connections {
        stream: HashSet<(Ident, String, Ident, String)>,
        message: HashSet<(Ident, String, Ident, String)>,
        blocks: HashSet<Ident>,
    },
    Done,
    Error(Option<Span>, String),
}

enum ConnectionResult {
    Stream,
    Message,
    Done,
    Error(Option<Span>, String),
}

fn next_connection(attrs: &mut Peekable<impl Iterator<Item = TokenTree>>) -> ConnectionResult {
    match attrs.next() {
        Some(TokenTree::Punct(p)) => {
            if p.to_string() == ";" {
                ConnectionResult::Done
            } else if p.to_string() == "|" {
                ConnectionResult::Message
            } else if p.to_string() == ">" {
                ConnectionResult::Stream
            } else {
                ConnectionResult::Error(
                    Some(p.span()),
                    "Exptected terminator (;), stream connector (>), or message connector (|)"
                        .into(),
                )
            }
        }
        Some(t) => ConnectionResult::Error(
            Some(t.span()),
            "Exptected terminator (;), stream connector (>), or message connector (|)".into(),
        ),
        None => ConnectionResult::Done,
    }
}

enum Connection {
    Stream,
    Message,
}

fn parse_connections(attrs: &mut Peekable<impl Iterator<Item = TokenTree>>) -> ParseResult {
    let mut blocks = HashSet::<Ident>::new();
    let mut stream = HashSet::<(Ident, String, Ident, String)>::new();
    let mut message = HashSet::<(Ident, String, Ident, String)>::new();

    let mut prev = match next_endpoint(attrs) {
        EndpointResult::Point(e) => e,
        EndpointResult::Error(span, string) => return ParseResult::Error(span, string),
        EndpointResult::Done => {
            return ParseResult::Done;
        }
    };
    blocks.insert(prev.0.clone());

    loop {
        let con = match next_connection(attrs) {
            ConnectionResult::Stream => Connection::Stream,
            ConnectionResult::Message => Connection::Message,
            ConnectionResult::Done => {
                return ParseResult::Connections {
                    stream,
                    message,
                    blocks,
                };
            }
            ConnectionResult::Error(span, string) => return ParseResult::Error(span, string),
        };

        let e = match next_endpoint(attrs) {
            EndpointResult::Point(e) => e,
            EndpointResult::Error(span, string) => return ParseResult::Error(span, string),
            EndpointResult::Done => {
                return ParseResult::Connections {
                    stream,
                    message,
                    blocks,
                }
            }
        };

        match con {
            Connection::Stream => {
                stream.insert((
                    prev.0,
                    prev.1.unwrap_or_else(|| "out".into()),
                    e.0.clone(),
                    e.1.clone().unwrap_or_else(|| "in".into()),
                ));
            }
            Connection::Message => {
                message.insert((
                    prev.0,
                    prev.1.unwrap_or_else(|| "out".into()),
                    e.0.clone(),
                    e.1.clone().unwrap_or_else(|| "in".into()),
                ));
            }
        }

        prev = e;
    }
}

struct Endpoint(Ident, Option<String>);

enum EndpointResult {
    Point(Endpoint),
    Error(Option<Span>, String),
    Done,
}

fn next_endpoint(attrs: &mut Peekable<impl Iterator<Item = TokenTree>>) -> EndpointResult {
    let block = match attrs.next() {
        Some(TokenTree::Ident(b)) => b,
        Some(t) => {
            return EndpointResult::Error(Some(t.span()), "Expected block identifier".into());
        }
        None => {
            return EndpointResult::Done;
        }
    };

    match attrs.peek() {
        Some(TokenTree::Punct(p)) => {
            if vec![";", ">", "|"].contains(&p.to_string().as_str()) {
                return EndpointResult::Point(Endpoint(block, None));
            } else if p.to_string() != "." {
                return EndpointResult::Error(
                    Some(p.span()),
                    "Expected dot or connection separator or terminator after block".into(),
                );
            } else {
                let _ = attrs.next();
            }
        }
        Some(t) => {
            return EndpointResult::Error(
                Some(t.span()),
                "Expected dot, connection separator, or terminator after block".into(),
            );
        }
        None => {
            return EndpointResult::Point(Endpoint(block, None));
        }
    }

    let port = match attrs.next() {
        Some(TokenTree::Ident(p)) => p.to_string(),
        Some(TokenTree::Literal(l)) => l.to_string().replace('"', ""),
        Some(t) => {
            return EndpointResult::Error(Some(t.span()), "Expected port identifier".into());
        }
        None => {
            return EndpointResult::Error(None, "Connections stopped unexpectedly".into());
        }
    };

    EndpointResult::Point(Endpoint(block, Some(port)))
}

//=========================================================================
// MESSAGE_HANDLER
//=========================================================================

/// Avoid boilerplate when creating message handlers.
///
/// Assume a block with a message handler that refers to a block function
/// `Self::my_handler`.
///
/// ```ignore
/// pub fn new() -> Block {
///     Block::new(
///         BlockMetaBuilder::new("MyBlock").build(),
///         StreamIoBuilder::new().build(),
///         MessageIoBuilder::new()
///             .add_input("handler", Self::my_handler)
///             .build(),
///         Self,
///     )
/// }
/// ```
///
/// The underlying machinery of the handler implementation is rather involved.
/// With the `message_handler` macro, it can be simplified to:
///
/// ```ignore
/// #[message_handler]
/// async fn my_handler(
///     &mut self,
///     _mio: &mut MessageIo<Self>,
///     _meta: &mut BlockMeta,
///     _p: Pmt,
/// ) -> Result<Pmt> {
///     Ok(Pmt::Null)
/// }
/// ```
#[proc_macro_attribute]
pub fn message_handler(
    _attr: proc_macro::TokenStream,
    fun: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let handler: syn::ItemFn = syn::parse(fun).unwrap();
    let mut out = TokenStream::new();

    let name = handler.sig.ident;
    let mio = get_parameter_ident(&handler.sig.inputs[1]).unwrap();
    let meta = get_parameter_ident(&handler.sig.inputs[2]).unwrap();
    let pmt = get_parameter_ident(&handler.sig.inputs[3]).unwrap();
    let body = handler.block.stmts;

    // println!("name {}", name);
    // println!("mio {}", mio);
    // println!("meta {}", meta);
    // println!("pmt {}", pmt);

    out.extend(quote! {
        fn #name<'a>(
            &'a mut self,
            #mio: &'a mut MessageIo<Self>,
            #meta: &'a mut BlockMeta,
            #pmt: Pmt,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Pmt>> + Send + 'a>> {
            use futuresdr::futures::FutureExt;
            async move {
                #(#body)*
            }.boxed()
        }
    });

    // println!("out: {}", out);
    out.into()
}

fn get_parameter_ident(arg: &syn::FnArg) -> Option<syn::Ident> {
    if let syn::FnArg::Typed(syn::PatType { pat, .. }) = arg {
        if let syn::Pat::Ident(ref i) = **pat {
            return Some(i.ident.clone());
        }
    }
    None
}
