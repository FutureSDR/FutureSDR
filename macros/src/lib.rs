use proc_macro2::Ident;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use proc_macro2::TokenTree;
use quote::quote;
use quote::quote_spanned;
use std::collections::HashSet;
use std::iter::Peekable;

enum ParseResult {
    Connections {
        stream: HashSet<(Ident, String, Ident, String)>,
        message: HashSet<(Ident, String, Ident, String)>,
        blocks: HashSet<Ident>,
    },
    Done,
    Error(Option<Span>, String),
}

#[proc_macro]
pub fn connect(attr: proc_macro::TokenStream) -> proc_macro::TokenStream {
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

    println!("code {}", out.to_string());
    out.into()
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
                return ConnectionResult::Done;
            } else if p.to_string() == "|" {
                return ConnectionResult::Message;
            } else if p.to_string() == ">" {
                return ConnectionResult::Stream;
            } else {
                return ConnectionResult::Error(
                    Some(p.span()),
                    "Exptected terminator (;), stream connector (>), or message connector (|)"
                        .into(),
                );
            }
        }
        Some(t) => {
            return ConnectionResult::Error(
                Some(t.span()),
                "Exptected terminator (;), stream connector (>), or message connector (|)".into(),
            );
        }
        None => {
            return ConnectionResult::Error(
                None,
                "Connections ended while looking for terminator (;), stream connector (>), or message connector (|)".into(),
            );
        }
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
        },
        None => {
            return EndpointResult::Done;
        },
    };

    match attrs.peek() {
        Some(TokenTree::Punct(p)) => {
            if vec![";", ">", "|"].contains(&p.to_string().as_str()) {
                return EndpointResult::Point(Endpoint(block, None));
            } else if p.to_string() != "." {
                return EndpointResult::Error(Some(p.span()), "Expected dot or connection separator or terminator after block".into());
            } else {
                let _ = attrs.next();
            }
        },
        Some(t) => {
            return EndpointResult::Error(Some(t.span()), "Expected dot, connection separator, or terminator after block".into());
        },
        None => {
            return EndpointResult::Error(None, "Connections stopped unexpectedly".into());
        },
    }

    let port = match attrs.next() {
        Some(TokenTree::Ident(p)) => p,
        Some(t) => {
            return EndpointResult::Error(Some(t.span()), "Expected port identifier".into());
        },
        None => {
            return EndpointResult::Error(None, "Connections stopped unexpectedly".into());
        },
    };

    EndpointResult::Point(Endpoint(block, Some(port.to_string())))
}
