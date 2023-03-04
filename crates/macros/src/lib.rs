//! Macros to make working with FutureSDR a bit nicer.

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
/// If a block uses non-standard port names it is possible to use triples, e.g.:
///
/// ```ignore
/// connect!(fg, src > input.foo.output > snk);
/// ```
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
///
/// Custom bufers for stream connections can be added by subsituding `>` with `[...]`
/// notation, e.g.:
///
/// ```ignore
/// connect!(fg, src [Slab::new()] snk);
/// ```
///
#[proc_macro]
pub fn connect(attr: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // println!("{}", attr.clone());
    // for a in attr.clone().into_iter() {
    //     println!("{:?}", a);
    // }
    let mut attrs = TokenStream::from(attr).into_iter().peekable();
    let mut out = TokenStream::new();

    let mut blocks = HashSet::<Ident>::new();
    let mut message_connections = Vec::<(Ident, String, Ident, String)>::new();
    let mut stream_connections = Vec::<(Ident, String, Ident, String, Option<TokenStream>)>::new();

    // search flowgraph variable
    let fg = match attrs.next() {
        Some(TokenTree::Ident(fg)) => fg,
        Some(t) => {
            return quote_spanned! {
                t.span() => compile_error!("Connect macro expects flowgraph as first argument.")
            }
            .into()
        }
        None => {
            return quote! {
                compile_error!("Connect macro expects flowgraph and connections as arguments.")
            }
            .into()
        }
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
                    stream_connections.push(c);
                }
                for c in message.into_iter() {
                    blocks.insert(c.0.clone());
                    blocks.insert(c.2.clone());
                    message_connections.push(c);
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

    out.extend(quote! {
        use futuresdr::runtime::Block;
        use futuresdr::runtime::Flowgraph;

        struct Foo;
        trait Add<T> {
            fn add(fg: &mut Flowgraph, b: T) -> usize;
        }
        impl Add<usize> for Foo {
            fn add(_fg: &mut Flowgraph, b: usize) -> usize {
                b
            }
        }
        impl Add<Block> for Foo {
            fn add(fg: &mut Flowgraph, b: Block) -> usize {
                fg.add_block(b)
            }
        }
    });

    // Add the blocks to the flowgraph
    for blk_id in blocks.clone() {
        out.extend(quote! {
            #[allow(unused_variables)]
            let #blk_id = Foo::add(&mut #fg, #blk_id);
        });
    }
    // Stream connections
    for (src, src_port, dst, dst_port, buffer) in stream_connections.into_iter() {
        let src_port = match src_port.parse::<usize>() {
            Ok(s) => quote!(#s),
            Err(_) => quote!(#src_port),
        };
        let dst_port = match dst_port.parse::<usize>() {
            Ok(s) => quote!(#s),
            Err(_) => quote!(#dst_port),
        };
        if let Some(b) = buffer {
            out.extend(quote! {
                #fg.connect_stream_with_type(#src, #src_port, #dst, #dst_port, #b)?;
            });
        } else {
            out.extend(quote! {
                #fg.connect_stream(#src, #src_port, #dst, #dst_port)?;
            });
        }
    }
    // Message connections
    for (src, src_port, dst, dst_port) in message_connections.into_iter() {
        let src_port = match src_port.parse::<usize>() {
            Ok(s) => quote!(#s),
            Err(_) => quote!(#src_port),
        };
        let dst_port = match dst_port.parse::<usize>() {
            Ok(s) => quote!(#s),
            Err(_) => quote!(#dst_port),
        };
        out.extend(quote! {
            #fg.connect_message(#src, #src_port, #dst, #dst_port)?;
        });
    }

    let b = blocks.clone().into_iter();
    out.extend(quote! {
            (#(#b),*)
    });

    let b = blocks.into_iter();
    let out = quote![
        #[allow(unused_variables)]
        let (#(#b),*) = {
            #out
        };
    ];

    // println!("code {}", out);
    out.into()
}

enum ParseResult {
    Connections {
        stream: Vec<(Ident, String, Ident, String, Option<TokenStream>)>,
        message: Vec<(Ident, String, Ident, String)>,
        blocks: HashSet<Ident>,
    },
    Done,
    Error(Option<Span>, String),
}

fn parse_connections(attrs: &mut Peekable<impl Iterator<Item = TokenTree>>) -> ParseResult {
    let mut blocks = HashSet::<Ident>::new();
    let mut stream = Vec::<(Ident, String, Ident, String, Option<TokenStream>)>::new();
    let mut message = Vec::<(Ident, String, Ident, String)>::new();

    let mut prev = match next_endpoint(attrs) {
        EndpointResult::Point(e) => e,
        EndpointResult::Error(span, string) => return ParseResult::Error(span, string),
        EndpointResult::Done => {
            return ParseResult::Done;
        }
    };
    blocks.insert(prev.block.clone());

    loop {
        enum Connection {
            Stream(Option<TokenStream>),
            Message,
        }

        let con = match next_connection(attrs) {
            ConnectionResult::Stream(r) => Connection::Stream(r),
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
            Connection::Stream(s) => {
                stream.push((prev.block, prev.output, e.block.clone(), e.input.clone(), s));
            }
            Connection::Message => {
                message.push((prev.block, prev.output, e.block.clone(), e.input.clone()));
            }
        }

        prev = e;
    }
}

struct Endpoint {
    block: Ident,
    input: String,
    output: String,
}

impl Endpoint {
    #[allow(clippy::new_ret_no_self)]
    fn new(block: Ident) -> EndpointResult {
        EndpointResult::Point(Self {
            block,
            input: "in".to_string(),
            output: "out".to_string(),
        })
    }

    fn with_port(block: Ident, port: TokenTree) -> EndpointResult {
        let i = match port {
            TokenTree::Ident(i) => i.to_string(),
            TokenTree::Literal(l) => l.to_string().replace('"', ""),
            _ => return EndpointResult::Error(None, format!("invalid endpoint port {}", port)),
        };
        EndpointResult::Point(Self {
            block,
            input: i.clone(),
            output: i,
        })
    }

    fn with_ports(block: Ident, in_port: TokenTree, out_port: TokenTree) -> EndpointResult {
        let input = match in_port {
            TokenTree::Ident(i) => i.to_string(),
            TokenTree::Literal(l) => l.to_string().replace('"', ""),
            _ => {
                return EndpointResult::Error(
                    None,
                    format!("invalid endpoint input port {}", in_port),
                )
            }
        };
        let output = match out_port {
            TokenTree::Ident(i) => i.to_string(),
            TokenTree::Literal(l) => l.to_string().replace('"', ""),
            _ => {
                return EndpointResult::Error(
                    None,
                    format!("invalid endpoint output port {}", out_port),
                )
            }
        };
        EndpointResult::Point(Self {
            block,
            input,
            output,
        })
    }
}

enum EndpointResult {
    Point(Endpoint),
    Error(Option<Span>, String),
    Done,
}

fn next_endpoint(attrs: &mut Peekable<impl Iterator<Item = TokenTree>>) -> EndpointResult {
    use TokenTree::*;

    let i1 = match attrs.next() {
        Some(Ident(s)) => Ident(s),
        Some(Literal(s)) => Literal(s),
        Some(t) => {
            return EndpointResult::Error(
                Some(t.span()),
                "Expected block identifier or port".into(),
            );
        }
        None => {
            return EndpointResult::Done;
        }
    };

    match (i1.clone(), attrs.peek()) {
        (Ident(i), Some(Punct(p))) => {
            if vec![";", ">", "|"].contains(&p.to_string().as_str()) {
                return Endpoint::new(i);
            } else if p.to_string() != "." {
                return EndpointResult::Error(
                    Some(p.span()),
                    "Expected dot or connection separator or terminator after block".into(),
                );
            } else {
                let _ = attrs.next();
            }
        }
        (Ident(i), Some(Group(_))) => return Endpoint::new(i),
        (_, Some(t)) => {
            return EndpointResult::Error(
                Some(t.span()),
                "Expected dot, connection separator, or terminator after block".into(),
            );
        }
        (Ident(i), None) => {
            return Endpoint::new(i);
        }
        (_, None) => {
            return EndpointResult::Error(None, "Endpoint consists only of string literal".into());
        }
    }

    let i2 = match attrs.next() {
        Some(TokenTree::Ident(p)) => TokenTree::Ident(p),
        Some(TokenTree::Literal(l)) => TokenTree::Literal(l),
        Some(t) => {
            return EndpointResult::Error(
                Some(t.span()),
                "Expected block or port identifier".into(),
            );
        }
        None => {
            return EndpointResult::Error(None, "Connections stopped unexpectedly".into());
        }
    };

    match (i1.clone(), attrs.peek()) {
        (Ident(i), Some(TokenTree::Punct(p))) => {
            if vec![";", ">", "|"].contains(&p.to_string().as_str()) {
                return Endpoint::with_port(i, i2);
            } else if p.to_string() != "." {
                return EndpointResult::Error(
                    Some(p.span()),
                    "Expected dot or connection separator or terminator after block".into(),
                );
            } else {
                let _ = attrs.next();
            }
        }
        (Ident(i), Some(TokenTree::Group(_))) => {
            return Endpoint::with_port(i, i2);
        }
        (_, Some(t)) => {
            return EndpointResult::Error(
                Some(t.span()),
                "Expected dot, connection separator, or terminator after block".into(),
            );
        }
        (TokenTree::Ident(i), None) => {
            return Endpoint::with_port(i, i2);
        }
        (_, None) => {
            return EndpointResult::Error(None, "Endpoint consists only of string literal".into());
        }
    }

    let i3 = match attrs.next() {
        Some(TokenTree::Ident(p)) => TokenTree::Ident(p),
        Some(TokenTree::Literal(l)) => TokenTree::Literal(l),
        Some(t) => {
            return EndpointResult::Error(Some(t.span()), "Expected port identifier".into());
        }
        None => {
            return EndpointResult::Error(None, "Connections stopped unexpectedly".into());
        }
    };

    match i2 {
        Ident(i) => Endpoint::with_ports(i, i1, i3),
        _ => EndpointResult::Error(
            None,
            "Middle token of endpoint triple should be the block Ident".into(),
        ),
    }
}

enum ConnectionResult {
    Stream(Option<TokenStream>),
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
                ConnectionResult::Stream(None)
            } else {
                ConnectionResult::Error(
                    Some(p.span()),
                    "Exptected terminator (;), stream connector (>), message connector (|), or custom buffer [..]"
                        .into(),
                )
            }
        }
        Some(TokenTree::Group(g)) => ConnectionResult::Stream(Some(g.stream())),
        Some(t) => ConnectionResult::Error(
            Some(t.span()),
            "Exptected terminator (;), stream connector (>), message connector (|), or custom buffer [..]".into(),
        ),
        None => ConnectionResult::Done,
    }
}

/// Avoid boilerplate when creating message handlers.
///
/// For technical reasons the `message_handler` macro for use inside and outside the
/// main crate need to be different. For the user this does not matter, since this
/// version gets re-exported as `futuresdr::macros::message_handler`.
///
/// See [`macro@message_handler_external`] for a more information on how to use the macro.
#[proc_macro_attribute]
pub fn message_handler(
    _attr: proc_macro::TokenStream,
    fun: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let handler: syn::ItemFn = syn::parse(fun).unwrap();
    let mut out = TokenStream::new();

    let name = handler.sig.ident;
    let io = get_parameter_ident(&handler.sig.inputs[1]).unwrap();
    let mio = get_parameter_ident(&handler.sig.inputs[2]).unwrap();
    let meta = get_parameter_ident(&handler.sig.inputs[3]).unwrap();
    let pmt = get_parameter_ident(&handler.sig.inputs[4]).unwrap();
    let body = handler.block.stmts;

    // println!("name {}", name);
    // println!("mio {}", mio);
    // println!("meta {}", meta);
    // println!("pmt {}", pmt);

    out.extend(quote! {
        fn #name<'a>(
            &'a mut self,
            #io: &'a mut WorkIo,
            #mio: &'a mut MessageIo<Self>,
            #meta: &'a mut BlockMeta,
            #pmt: Pmt,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Pmt>> + Send + 'a>> {
            use crate::futures::FutureExt;
            async move {
                #(#body)*
            }.boxed()
        }
    });

    // println!("out: {}", out);
    out.into()
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
///     _io: &mut WorkIo,
///     _mio: &mut MessageIo<Self>,
///     _meta: &mut BlockMeta,
///     _p: Pmt,
/// ) -> Result<Pmt> {
///     Ok(Pmt::Null)
/// }
/// ```
#[proc_macro_attribute]
pub fn message_handler_external(
    _attr: proc_macro::TokenStream,
    fun: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let handler: syn::ItemFn = syn::parse(fun).unwrap();
    let mut out = TokenStream::new();

    let name = handler.sig.ident;
    let io = get_parameter_ident(&handler.sig.inputs[1]).unwrap();
    let mio = get_parameter_ident(&handler.sig.inputs[2]).unwrap();
    let meta = get_parameter_ident(&handler.sig.inputs[3]).unwrap();
    let pmt = get_parameter_ident(&handler.sig.inputs[4]).unwrap();
    let body = handler.block.stmts;

    // println!("name {}", name);
    // println!("mio {}", mio);
    // println!("meta {}", meta);
    // println!("pmt {}", pmt);

    out.extend(quote! {
        fn #name<'a>(
            &'a mut self,
            #io: &'a mut WorkIo,
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
