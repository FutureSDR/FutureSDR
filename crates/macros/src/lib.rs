//! Procedural macros for FutureSDR applications and custom blocks.
//!
//! The main entry points are:
//!
//! - `connect!`, which adds blocks to a flowgraph and wires stream, local
//!   stream, and message connections.
//! - `#[derive(Block)]`, which generates the runtime interface for block kernels.
use proc_macro::TokenStream;
use quote::quote;
use syn::Attribute;
use syn::Data;
use syn::DeriveInput;
use syn::Fields;
use syn::GenericArgument;
use syn::GenericParam;
use syn::Ident;
use syn::Index;
use syn::Meta;
use syn::PathArguments;
use syn::Result;
use syn::Token;
use syn::Type;
use syn::bracketed;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse_macro_input;
use syn::parse_quote;
use syn::punctuated::Punctuated;
use syn::token;

/// Avoid boilerplate when setting up the flowgraph.
///
/// `connect!` adds all mentioned blocks to the flowgraph if needed and then
/// records the requested connections. It leaves the local variable names bound
/// to typed block references, so they can be used later for inspection or
/// message calls.
///
/// ```ignore
/// let mut fg = Flowgraph::new();
///
/// connect!(fg,
///     src.out > shift.in;
///     shift > resamp1 > demod;
///     demod > resamp2 > snk;
/// );
/// ```
///
/// It roughly generates code like:
///
/// ```ignore
/// // Add all the blocks to the `Flowgraph`...
/// let src = fg.add(src)?;
/// let shift = fg.add(shift)?;
/// let resamp1 = fg.add(resamp1)?;
/// let demod = fg.add(demod)?;
/// let resamp2 = fg.add(resamp2)?;
/// let snk = fg.add(snk)?;
///
/// // ... and connect the ports appropriately
/// fg.stream(&src, |b| b.output(), &shift, |b| b.input())?;
/// fg.stream(&shift, |b| b.output(), &resamp1, |b| b.input())?;
/// fg.stream(&resamp1, |b| b.output(), &demod, |b| b.input())?;
/// fg.stream(&demod, |b| b.output(), &resamp2, |b| b.input())?;
/// fg.stream(&resamp2, |b| b.output(), &snk, |b| b.input())?;
/// ```
///
/// Connection endpoints are defined by `block.port_name`. Standard stream port
/// names can be omitted: a missing source port means `output()`, and a missing
/// destination port means `input()`. Message endpoints default to `"out"` and
/// `"in"`.
///
/// Send-capable stream connections are indicated as `>`, while local-domain-only
/// stream connections for non-`Send` buffers are indicated as `~>`. Message
/// connections are indicated as `|`.
///
/// If a block uses non-standard port names it is possible to use triples, e.g.:
///
/// ```ignore
/// connect!(fg, src > input.foo.output > snk);
/// ```
///
/// Indexed stream ports generated from `Vec<T>` or `[T; N]` fields can be
/// selected with `field[index]`:
///
/// ```ignore
/// connect!(fg, src.output[0] > input.snk);
/// ```
///
/// It is possible to add blocks that have no connections by just putting them
/// on a line separately.
///
/// ```ignore
/// connect!(fg, dummy);
/// ```
#[proc_macro]
pub fn connect(input: TokenStream) -> TokenStream {
    let connect_input = parse_macro_input!(input as ConnectInput);
    generate_connect(connect_input, ConnectMode::BlockingNative).into()
}

/// Async counterpart to [`connect!`].
///
/// This macro emits `.await` points for the generated connection operations and
/// is the required flowgraph-construction macro on WASM targets.
#[proc_macro]
pub fn connect_async(input: TokenStream) -> TokenStream {
    let connect_input = parse_macro_input!(input as ConnectInput);
    generate_connect(connect_input, ConnectMode::Async).into()
}

#[derive(Clone, Copy)]
enum ConnectMode {
    Async,
    BlockingNative,
}

fn generate_connect(connect_input: ConnectInput, mode: ConnectMode) -> proc_macro2::TokenStream {
    // dbg!(&connect_input);
    let fg = connect_input.flowgraph;

    let mut blocks: Vec<Ident> = Vec::new();
    let mut connections = Vec::new();

    // Collect all blocks and generate connections
    for conn in connect_input.connection_strings.iter() {
        let src_block = &conn.source.block;
        blocks.push(src_block.clone());

        let mut src_block = &conn.source.block;
        let mut src_port = &conn.source.output;

        for (connection_type, dst) in &conn.connections {
            blocks.push(dst.block.clone());

            let out = match connection_type {
                ConnectionType::Stream | ConnectionType::LocalStream => {
                    let src_port = port_method(src_port, quote!(output()));
                    let dst_port = port_method(&dst.input, quote!(input()));
                    let dst_block = &dst.block;
                    let method = match connection_type {
                        ConnectionType::Stream => quote! { stream_async },
                        ConnectionType::LocalStream => quote! { stream_local_async },
                        _ => unreachable!(),
                    };
                    quote! {
                        #fg.#method(
                            &#src_block,
                            |b| b.#src_port,
                            &#dst_block,
                            |b| b.#dst_port,
                        ).await?;
                    }
                }
                ConnectionType::Message => {
                    let src_port = if let Some(p) = &src_port {
                        let src_port = p.name.to_string();
                        quote! { #src_port }
                    } else {
                        quote!("out")
                    };
                    let dst_port = if let Some(p) = &dst.input {
                        let dst_port = p.name.to_string();
                        quote! { #dst_port }
                    } else {
                        quote!("in")
                    };
                    let dest_block = &dst.block;
                    quote! {
                        #fg.message_async(
                            #src_block,
                            #src_port,
                            #dest_block,
                            #dst_port,
                        ).await?;
                    }
                }
            };
            connections.push(out);
            src_block = &dst.block;
            src_port = &dst.output;
        }
    }

    // Deduplicate blocks
    blocks.sort_by_key(|b| b.to_string());
    blocks.dedup();

    let block_decls = blocks.iter().map(|block| {
        quote! {
            let #block = #fg.add_to_flowgraph(#block).await?;
        }
    });

    let add_to_flowgraph_trait =
        quote! { use ::futuresdr::runtime::__private::AddToFlowgraph as _; };

    let body = quote! {
        #add_to_flowgraph_trait
        #(#block_decls)*
        #(#connections)*
        ::core::result::Result::Ok::<_, ::futuresdr::runtime::Error>((#(#blocks),*))
    };

    match mode {
        ConnectMode::Async => quote![
            #[allow(unused_variables)]
            let (#(#blocks),*) = {
                #body
            }?;
        ],
        ConnectMode::BlockingNative => quote![
            #[cfg(target_arch = "wasm32")]
            compile_error!("connect! is synchronous and unavailable on wasm32; use connect_async!(...).await instead");

            #[cfg(not(target_arch = "wasm32"))]
            #[allow(unused_variables)]
            let (#(#blocks),*) = ::futuresdr::runtime::block_on(async {
                #body
            })?;
        ],
    }
}

fn port_method(port: &Option<Port>, default: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    match port {
        Some(Port { name, index: None }) => {
            quote! { #name() }
        }
        Some(Port {
            name,
            index: Some(i),
        }) => {
            quote! { #name().get_mut(#i).unwrap() }
        }
        None => default,
    }
}

// full macro input
#[derive(Debug)]
struct ConnectInput {
    flowgraph: Ident,
    _comma: Token![,],
    connection_strings: Punctuated<ConnectionString, Token![;]>,
}
impl Parse for ConnectInput {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(ConnectInput {
            flowgraph: input.parse()?,
            _comma: input.parse()?,
            connection_strings: Punctuated::parse_terminated(input)?,
        })
    }
}

// connection line in the macro input
#[derive(Debug)]
struct ConnectionString {
    source: Source,
    connections: Vec<(ConnectionType, Endpoint)>,
}
impl Parse for ConnectionString {
    fn parse(input: ParseStream) -> Result<Self> {
        let source: Source = input.parse()?;
        let mut connections = Vec::new();

        while !input.is_empty() && !input.peek(Token![;]) {
            if input.peek(Token![<]) {
                return Err(input.error(
                    "the `<` circuit-close operator was removed; in-place buffers recycle when dropped",
                ));
            }
            let ct: ConnectionType = input.parse()?;
            let dest: Endpoint = input.parse()?;
            connections.push((ct, dest));
        }

        Ok(ConnectionString {
            source,
            connections,
        })
    }
}

#[derive(Debug)]
enum ConnectionType {
    Stream,
    LocalStream,
    Message,
}

impl Parse for ConnectionType {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![>]) {
            input.parse::<Token![>]>()?;
            Ok(Self::Stream)
        } else if input.peek(Token![~]) {
            input.parse::<Token![~]>()?;
            input.parse::<Token![>]>()?;
            Ok(Self::LocalStream)
        } else if input.peek(Token![|]) {
            input.parse::<Token![|]>()?;
            Ok(Self::Message)
        } else {
            Err(input.error("expected `>`, `~>`, or `|` to specify the connection type"))
        }
    }
}

#[derive(Debug)]
struct Source {
    block: Ident,
    output: Option<Port>,
}
impl Parse for Source {
    fn parse(input: ParseStream) -> Result<Self> {
        let block: Ident = input.parse()?;
        if input.peek(Token![.]) {
            input.parse::<Token![.]>()?;
            let port: Port = input.parse()?;
            Ok(Self {
                block,
                output: Some(port),
            })
        } else {
            Ok(Self {
                block,
                output: None,
            })
        }
    }
}

// connection endpoint is a block with input and output ports
#[derive(Debug)]
struct Endpoint {
    block: Ident,
    input: Option<Port>,
    output: Option<Port>,
}
impl Parse for Endpoint {
    fn parse(input: ParseStream) -> Result<Self> {
        let first: Port = input.parse()?;

        // there is only one identifier, it has to be the block
        if !input.peek(Token![.]) {
            if first.index.is_none() {
                return Ok(Self {
                    block: first.name,
                    input: None,
                    output: None,
                });
            } else {
                return Err(input.error("expected endpoint, got only port"));
            }
        }

        input.parse::<Token![.]>()?;
        let block: Ident = input.parse()?;

        if !input.peek(Token![.]) {
            return Ok(Self {
                block,
                input: Some(first),
                output: None,
            });
        }

        input.parse::<Token![.]>()?;
        let second: Port = input.parse()?;

        Ok(Self {
            block,
            input: Some(first),
            output: Some(second),
        })
    }
}

// input or output port
#[derive(Debug)]
struct Port {
    name: Ident,
    index: Option<Index>,
}
impl Parse for Port {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;
        let index = if input.peek(token::Bracket) {
            let content;
            bracketed!(content in input);
            Some(content.parse()?)
        } else {
            None
        };
        Ok(Port { name, index })
    }
}

/// Check for  `#[input]` attribute
fn has_input_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("input"))
}
/// Check for  `#[output]` attribute
fn has_output_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("output"))
}
/// Check if parameter is a Vec
fn is_vec(type_path: &syn::TypePath) -> bool {
    if type_path.path.segments.len() != 1 {
        return false;
    }

    let segment = &type_path.path.segments[0];
    if segment.ident != "Vec" {
        return false;
    }

    matches!(segment.arguments, PathArguments::AngleBracketed(_))
}

fn port_bound_types(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Path(type_path) if is_vec(type_path) => {
            if let PathArguments::AngleBracketed(args) = &type_path.path.segments[0].arguments {
                args.args
                    .iter()
                    .filter_map(|arg| match arg {
                        GenericArgument::Type(ty) => Some(ty.clone()),
                        _ => None,
                    })
                    .collect()
            } else {
                Vec::new()
            }
        }
        Type::Array(array) => vec![(*array.elem).clone()],
        Type::Tuple(tuple) => tuple.elems.iter().cloned().collect(),
        _ => vec![ty.clone()],
    }
}

//=========================================================================
// BLOCK MACRO
//=========================================================================
/// Derive the runtime interface for a block kernel.
///
/// `#[derive(Block)]` is used on a struct that implements `Kernel`. Fields
/// marked with `#[input]` or `#[output]` become stream ports. Struct-level
/// `#[message_inputs(...)]` and `#[message_outputs(...)]` attributes declare
/// message ports.
///
/// ```ignore
/// #[derive(Block)]
/// #[message_inputs(set_gain)]
/// #[message_outputs(done)]
/// struct Scale {
///     #[input]
///     input: DefaultCpuReader<f32>,
///     #[output]
///     output: DefaultCpuWriter<f32>,
///     gain: f32,
/// }
/// ```
///
/// Generated stream port getter methods have the same names as the annotated
/// fields and are used by the `connect!` macro. `Vec<T>` and arrays of buffer
/// ports are expanded into indexed port names such as `outputs[0]`; tuples are
/// exposed through dynamic port names such as `ports.1`.
///
/// Supported struct attributes:
///
/// - `#[message_inputs(handler)]`: call `self.handler(...)` for a message input
///   named `handler`.
/// - `#[message_inputs(handler = "port-name")]`: expose a message input under a
///   name that differs from the Rust method name.
/// - `#[message_outputs(out)]`: declare a message output port named `out`.
/// - `#[blocking]`: run this block on the blocking/local execution path.
/// - `#[type_name(Name)]`: override the type name exposed in runtime
///   descriptions.
/// - `#[null_kernel]`: generate an empty `Kernel` implementation.
#[proc_macro_derive(
    Block,
    attributes(
        input,
        output,
        message_inputs,
        message_outputs,
        blocking,
        type_name,
        null_kernel
    )
)]
pub fn derive_block(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    derive_block_impl(input)
}

fn derive_block_impl(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let generics = &input.generics;
    let where_clause = &input.generics.where_clause;

    let mut message_inputs: Vec<Ident> = Vec::new();
    let mut message_input_names: Vec<String> = Vec::new();
    let mut message_output_names: Vec<String> = Vec::new();
    let mut kernel = quote! {};
    let mut blocking = quote! { false };
    let mut type_name = struct_name.to_string();

    // remove defaults from generics
    let mut generics = generics.clone();
    for param in &mut generics.params {
        match param {
            GenericParam::Type(type_param) => {
                type_param.default = None;
            }
            GenericParam::Const(const_param) => {
                const_param.default = None;
            }
            GenericParam::Lifetime(_) => {}
        }
    }

    let unconstraint_params: Vec<proc_macro2::TokenStream> = generics
        .params
        .iter()
        .map(|param| match param {
            GenericParam::Type(ty) => {
                let ident = &ty.ident;
                quote! { #ident }
            }
            GenericParam::Lifetime(lt) => {
                let lifetime = &lt.lifetime;
                quote! { #lifetime }
            }
            GenericParam::Const(c) => {
                let ident = &c.ident;
                quote! { #ident }
            }
        })
        .collect();

    // Surround the parameters with angle brackets if they exist
    let unconstraint_generics = if generics.params.is_empty() {
        quote! {}
    } else {
        quote! { <#(#unconstraint_params),*> }
    };

    // Parse Struct
    let struct_data = match input.data {
        Data::Struct(data) => data,
        _ => {
            return syn::Error::new_spanned(input.ident, "Block can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    let stream_inputs = match struct_data.fields {
        Fields::Named(ref fields) => {
            fields
                .named
                .iter()
                .filter_map(|field| {
                    // Check if field has #[input] attribute
                    if !field.attrs.iter().any(|attr| attr.path().is_ident("input")) {
                        return None;
                    }

                    let field_name = field.ident.as_ref().unwrap();
                    let field_name_str = field_name.to_string();

                    match &field.ty {
                        // Handle Vec<T>
                        Type::Path(type_path) if is_vec(type_path) => {
                            let name_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    f(PortId::new(format!("{}[{}]", #field_name_str, i)), &mut self.#field_name[i])?;
                                }
                            };
                            let init_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    __FsdrInput::init_from(&mut self.#field_name[i], block_id, PortId::new(format!("{}[{}]", #field_name_str, i)), &inboxes);
                                }
                            };
                            let validate_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    __FsdrInput::validate(&self.#field_name[i])?;
                                }
                            };
                            let notify_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    __FsdrInput::notify_finished(&mut self.#field_name[i]).await;
                                }
                            };
                            let finish_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if port == format!("{}[{}]", #field_name_str, i) {
                                        __FsdrInput::finish(&mut self.#field_name[i]);
                                        return Ok(());
                                    }
                                }
                            };
                            let get_input_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if name == format!("{}[{}]", #field_name_str, i) {
                                        return Ok(f(&mut self.#field_name[i]));
                                    }
                                }
                            };
                            Some((name_code, init_code, validate_code, notify_code, finish_code, get_input_code))
                        }
                        // Handle arrays [T; N]
                        Type::Array(array) => {
                            let len = &array.len;
                            let name_code = quote! {
                                for i in 0..#len {
                                    f(PortId::new(format!("{}[{}]", #field_name_str, i)), &mut self.#field_name[i])?;
                                }
                            };
                            let init_code = quote! {
                                for i in 0..#len {
                                    __FsdrInput::init_from(&mut self.#field_name[i], block_id, PortId::new(format!("{}[{}]", #field_name_str, i)), &inboxes);
                                }
                            };
                            let validate_code = quote! {
                                for i in 0..#len {
                                    __FsdrInput::validate(&self.#field_name[i])?;
                                }
                            };
                            let notify_code = quote! {
                                for i in 0..#len {
                                    __FsdrInput::notify_finished(&mut self.#field_name[i]).await;
                                }
                            };
                            let finish_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if port == format!("{}[{}]", #field_name_str, i) {
                                        __FsdrInput::finish(&mut self.#field_name[i]);
                                        return Ok(());
                                    }
                                }
                            };
                            let get_input_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if name == format!("{}[{}]", #field_name_str, i) {
                                        return Ok(f(&mut self.#field_name[i]));
                                    }
                                }
                            };
                            Some((name_code, init_code, validate_code, notify_code, finish_code, get_input_code))
                        }
                        // Handle tuples (T1, T2, ...)
                        Type::Tuple(tuple) => {
                            let name_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    f(PortId::new(format!("{}.{}", #field_name_str, #index)), &mut self.#field_name.#index)?;
                                }
                            });
                            let name_code = quote! {
                                #(#name_code)*
                            };
                            let init_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    __FsdrInput::init_from(&mut self.#field_name.#index, block_id, PortId::new(format!("{}.{}", #field_name_str, #index)), &inboxes);
                                }
                            });
                            let init_code = quote! {
                                #(#init_code)*
                            };
                            let validate_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    __FsdrInput::validate(&self.#field_name.#index)?;
                                }
                            });
                            let validate_code = quote! {
                                #(#validate_code)*
                            };
                            let notify_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    __FsdrInput::notify_finished(&mut self.#field_name.#index).await;
                                }
                            });
                            let notify_code = quote! {
                                #(#notify_code)*
                            };
                            let finish_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote!{
                                    if port == format!("{}.{}", #field_name_str, #index) {
                                        __FsdrInput::finish(&mut self.#field_name.#index);
                                        return Ok(());
                                    }
                                }
                            });
                            let finish_code = quote! {
                                #(#finish_code)*
                            };
                            let get_input_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote!{
                                    if name == format!("{}.{}", #field_name_str, #index) {
                                        return Ok(f(&mut self.#field_name.#index));
                                    }
                                }
                            });
                            let get_input_code = quote! {
                                #(#get_input_code)*
                            };
                            Some((name_code, init_code, validate_code, notify_code, finish_code, get_input_code))
                        }
                        // Handle normal types
                        _ => {
                            let name_code = quote! {
                                f(PortId::new(#field_name_str.to_string()), &mut self.#field_name)?;
                            };
                            let init_code = quote! {
                                __FsdrInput::init_from(&mut self.#field_name, block_id, PortId::new(#field_name_str.to_string()), &inboxes);
                            };
                            let validate_code = quote! {
                                __FsdrInput::validate(&self.#field_name)?;
                            };
                            let notify_code = quote! {
                                __FsdrInput::notify_finished(&mut self.#field_name).await;
                            };
                            let finish_code = quote! {
                                if port == #field_name_str {
                                    __FsdrInput::finish(&mut self.#field_name);
                                    return Ok(());
                                }
                            };
                            let get_input_code = quote! {
                                if name == #field_name_str {
                                    return Ok(f(&mut self.#field_name));
                                }
                            };
                            Some((name_code, init_code, validate_code, notify_code, finish_code, get_input_code))
                        }
                    }
                })
                .collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };

    let stream_inputs_visit = stream_inputs
        .iter()
        .map(|x| x.0.clone())
        .collect::<Vec<_>>();
    let stream_inputs_notify = stream_inputs
        .iter()
        .map(|x| x.3.clone())
        .collect::<Vec<_>>();
    let stream_inputs_get = stream_inputs
        .iter()
        .map(|x| x.5.clone())
        .collect::<Vec<_>>();

    let stream_outputs = match struct_data.fields {
        Fields::Named(ref fields) => {
            fields
                .named
                .iter()
                .filter_map(|field| {
                    // Check if field has #[input] attribute
                    if !field.attrs.iter().any(|attr| attr.path().is_ident("output")) {
                        return None;
                    }

                    let field_name = field.ident.as_ref().unwrap();
                    let field_name_str = field_name.to_string();

                    match &field.ty {
                        // Handle Vec<T>
                        Type::Path(type_path) if is_vec(type_path) => {
                            let name_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    f(PortId::new(format!("{}[{}]", #field_name_str, i)), &mut self.#field_name[i])?;
                                }
                            };
                            let init_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    __FsdrOutput::init_from(&mut self.#field_name[i], block_id, PortId::new(format!("{}[{}]", #field_name_str, i)), &inboxes);
                                }
                            };
                            let validate_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    __FsdrOutput::validate(&self.#field_name[i])?;
                                }
                            };
                            let notify_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    __FsdrOutput::notify_finished(&mut self.#field_name[i]).await;
                                }
                            };
                            let connect_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if name == format!("{}[{}]", #field_name_str, i) {
                                        return Ok(f(&mut self.#field_name[i]));
                                    }
                                }
                            };
                            Some((name_code, init_code, validate_code, notify_code, connect_code))
                        }
                        // Handle arrays [T; N]
                        Type::Array(array) => {
                            let len = &array.len;
                            let name_code = quote! {
                                for i in 0..#len {
                                    f(PortId::new(format!("{}[{}]", #field_name_str, i)), &mut self.#field_name[i])?;
                                }
                            };
                            let init_code = quote! {
                                for i in 0..#len {
                                    __FsdrOutput::init_from(&mut self.#field_name[i], block_id, PortId::new(format!("{}[{}]", #field_name_str, i)), &inboxes);
                                }
                            };
                            let validate_code = quote! {
                                for i in 0..#len {
                                    __FsdrOutput::validate(&self.#field_name[i])?;
                                }
                            };
                            let notify_code = quote! {
                                for i in 0..#len {
                                    __FsdrOutput::notify_finished(&mut self.#field_name[i]).await;
                                }
                            };
                            let connect_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if name == format!("{}[{}]", #field_name_str, i) {
                                        return Ok(f(&mut self.#field_name[i]));
                                    }
                                }
                            };
                            Some((name_code, init_code, validate_code, notify_code, connect_code))
                        }
                        // Handle tuples (T1, T2, ...)
                        Type::Tuple(tuple) => {
                            let name_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    f(PortId::new(format!("{}.{}", #field_name_str, #index)), &mut self.#field_name.#index)?;
                                }
                            });
                            let name_code = quote! {
                                #(#name_code)*
                            };
                            let init_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    __FsdrOutput::init_from(&mut self.#field_name.#index, block_id, PortId::new(format!("{}.{}", #field_name_str, #index)), &inboxes);
                                }
                            });
                            let init_code = quote! {
                                #(#init_code)*
                            };
                            let validate_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    __FsdrOutput::validate(&self.#field_name.#index)?;
                                }
                            });
                            let validate_code = quote! {
                                #(#validate_code)*
                            };
                            let notify_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    __FsdrOutput::notify_finished(&mut self.#field_name.#index).await;
                                }
                            });
                            let notify_code = quote! {
                                #(#notify_code)*
                            };
                            let connect_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote!{
                                    if name == format!("{}.{}", #field_name_str, #index) {
                                        return Ok(f(&mut self.#field_name.#index));
                                    }
                                }
                            });
                            let connect_code = quote! {
                                #(#connect_code)*
                            };
                            Some((name_code, init_code, validate_code, notify_code, connect_code))
                        }
                        // Handle normal types
                        _ => {
                            let name_code = quote! {
                                f(PortId::new(#field_name_str.to_string()), &mut self.#field_name)?;
                            };
                            let init_code = quote! {
                                __FsdrOutput::init_from(&mut self.#field_name, block_id, PortId::new(#field_name_str.to_string()), &inboxes);
                            };
                            let validate_code = quote! {
                                __FsdrOutput::validate(&self.#field_name)?;
                            };
                            let notify_code = quote! {
                                __FsdrOutput::notify_finished(&mut self.#field_name).await;
                            };
                            let connect_code = quote! {
                                if name == #field_name_str {
                                    return Ok(f(&mut self.#field_name));
                                }
                            };
                            Some((name_code, init_code, validate_code, notify_code, connect_code))
                        }
                    }
                })
                .collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };

    let stream_outputs_visit = stream_outputs
        .iter()
        .map(|x| x.0.clone())
        .collect::<Vec<_>>();
    let stream_outputs_notify = stream_outputs
        .iter()
        .map(|x| x.3.clone())
        .collect::<Vec<_>>();
    let stream_outputs_get = stream_outputs
        .iter()
        .map(|x| x.4.clone())
        .collect::<Vec<_>>();

    // Collect the names and types of fields that have the #[input] or #[output] attribute
    let (port_idents, port_types): (Vec<Ident>, Vec<Type>) = match struct_data.fields {
        Fields::Named(ref fields_named) => fields_named
            .named
            .iter()
            .filter_map(|field| {
                if has_input_attr(&field.attrs) || has_output_attr(&field.attrs) {
                    let ident = field.ident.clone().unwrap();
                    let ty = field.ty.clone();
                    Some((ident, ty))
                } else {
                    None
                }
            })
            .unzip(),
        Fields::Unnamed(_) | Fields::Unit => (Vec::new(), Vec::new()),
    };
    let port_getter_fns = port_idents
        .iter()
        .zip(port_types.iter())
        .map(|(ident, ty)| {
            quote! {
                /// Getter for stream port.
                pub fn #ident(&mut self) -> &mut #ty {
                    &mut self.#ident
                }
            }
        });

    let input_bound_types = match struct_data.fields {
        Fields::Named(ref fields_named) => fields_named
            .named
            .iter()
            .filter(|field| has_input_attr(&field.attrs))
            .flat_map(|field| port_bound_types(&field.ty))
            .collect::<Vec<_>>(),
        Fields::Unnamed(_) | Fields::Unit => Vec::new(),
    };
    let output_bound_types = match struct_data.fields {
        Fields::Named(ref fields_named) => fields_named
            .named
            .iter()
            .filter(|field| has_output_attr(&field.attrs))
            .flat_map(|field| port_bound_types(&field.ty))
            .collect::<Vec<_>>(),
        Fields::Unnamed(_) | Fields::Unit => Vec::new(),
    };
    let mut kernel_interface_generics = generics.clone();
    {
        let where_clause = kernel_interface_generics.make_where_clause();
        for ty in input_bound_types.iter() {
            where_clause
                .predicates
                .push(parse_quote!(#ty: ::futuresdr::runtime::buffer::BufferReader));
        }
        for ty in output_bound_types.iter() {
            where_clause
                .predicates
                .push(parse_quote!(#ty: ::futuresdr::runtime::buffer::BufferWriter));
            where_clause.predicates.push(parse_quote!(
                <#ty as ::futuresdr::runtime::buffer::BufferWriter>::Mode:
                    ::futuresdr::runtime::buffer::BufferWriterTokenPolicy<#ty>
            ));
        }
    }
    let (kernel_interface_impl_generics, _, kernel_interface_where_clause) =
        kernel_interface_generics.split_for_impl();

    // Search for struct attributes
    for attr in &input.attrs {
        if attr.path().is_ident("message_inputs") {
            let nested = attr
                .parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )
                .unwrap();
            for m in nested {
                match m {
                    Meta::NameValue(m) => {
                        message_inputs.push(m.path.get_ident().unwrap().clone());
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(s),
                            ..
                        }) = m.value
                        {
                            message_input_names.push(s.value());
                        } else {
                            panic!(
                                "message handlers have to be an identifier or identifier = \"port name\""
                            );
                        }
                    }
                    Meta::Path(p) => {
                        let p = p.get_ident().unwrap();
                        message_inputs.push(p.clone());
                        message_input_names.push(p.to_string());
                    }
                    _ => {
                        panic!("message inputs has to be a list of name-values or paths")
                    }
                }
            }
        } else if attr.path().is_ident("message_outputs") {
            let nested = attr
                .parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )
                .unwrap();
            for m in nested {
                match m {
                    Meta::Path(p) => {
                        let p = p.get_ident().unwrap();
                        message_output_names.push(p.to_string());
                    }
                    _ => {
                        panic!("message outputs has to be a list of paths")
                    }
                }
            }
        } else if attr.path().is_ident("null_kernel") {
            let kernel_trait = quote! { ::futuresdr::runtime::dev::Kernel };
            kernel = quote! {
                #[doc(hidden)]
                impl #generics #kernel_trait for #struct_name #generics
                    #where_clause { }

            }
        } else if attr.path().is_ident("blocking") {
            blocking = quote! { true }
        } else if attr.path().is_ident("type_name") {
            let nested = attr
                .parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )
                .unwrap();
            if let Some(Meta::Path(p)) = nested.get(0) {
                type_name = p.get_ident().unwrap().to_string();
            } else {
                panic!("type_name attribute should be in the form type_name(foo)");
            }
        }
    }

    // Generate handler names as strings
    let message_input_names = message_input_names
        .into_iter()
        .map(|handler| {
            let handler = if let Some(stripped) = handler.strip_prefix("r#") {
                stripped.to_string()
            } else {
                handler
            };
            quote! {
                #handler
            }
        })
        .collect::<Vec<_>>();

    // Generate match arms for the handle method
    let handler_matches = message_inputs
        .iter()
        .zip(message_input_names.clone())
        .map(|(handler, handler_name)| {
            quote! {
                #handler_name  => self.#handler(io, mo, meta, p).await,
            }
        })
        .collect::<Vec<_>>();

    let interface_trait = quote! { ::futuresdr::runtime::__private::KernelInterface };
    let work_io_type = quote! { ::futuresdr::runtime::dev::WorkIo };
    let port_getters = quote! {
        impl #generics #struct_name #unconstraint_generics
            #where_clause
        {
            #(#port_getter_fns)*
        }
    };

    let expanded = quote! {

        #port_getters

        impl #kernel_interface_impl_generics #interface_trait for #struct_name #unconstraint_generics
            #kernel_interface_where_clause
        {
            fn is_blocking() -> bool {
                #blocking
            }
            fn type_name() -> &'static str {
                static TYPE_NAME: &str = #type_name;
                TYPE_NAME
            }
            fn visit_stream_inputs(
                &mut self,
                f: &mut dyn FnMut(
                    ::futuresdr::runtime::PortId,
                    &mut dyn ::futuresdr::runtime::buffer::DynBufferReader,
                ) -> ::futuresdr::runtime::Result<(), ::futuresdr::runtime::Error>,
            ) -> ::futuresdr::runtime::Result<(), ::futuresdr::runtime::Error> {
                use ::futuresdr::runtime::PortId;
                #(#stream_inputs_visit)*
                Ok(())
            }

            fn visit_stream_outputs(
                &mut self,
                f: &mut dyn FnMut(
                    ::futuresdr::runtime::PortId,
                    &mut dyn ::futuresdr::runtime::buffer::DynBufferWriter,
                ) -> ::futuresdr::runtime::Result<(), ::futuresdr::runtime::Error>,
            ) -> ::futuresdr::runtime::Result<(), ::futuresdr::runtime::Error> {
                use ::futuresdr::runtime::PortId;
                #(#stream_outputs_visit)*
                Ok(())
            }

            fn with_stream_input<'a, R>(
                &'a mut self,
                id: &::futuresdr::runtime::PortId,
                f: impl FnOnce(&'a mut dyn ::futuresdr::runtime::buffer::DynBufferReader) -> R,
            ) -> ::futuresdr::runtime::Result<R, ::futuresdr::runtime::Error> {
                use ::futuresdr::runtime::Error;
                use ::futuresdr::runtime::BlockPortCtx;
                let name = id.name();
                #(#stream_inputs_get)*
                Err(Error::InvalidStreamPort(BlockPortCtx::None, id.clone()))
            }

            fn with_stream_output<'a, R>(
                &'a mut self,
                id: &::futuresdr::runtime::PortId,
                f: impl FnOnce(&'a mut dyn ::futuresdr::runtime::buffer::DynBufferWriter) -> R,
            ) -> ::futuresdr::runtime::Result<R, ::futuresdr::runtime::Error> {
                use ::futuresdr::runtime::Error;
                use ::futuresdr::runtime::BlockPortCtx;
                let name = id.name();
                #(#stream_outputs_get)*
                Err(Error::InvalidStreamPort(BlockPortCtx::None, id.clone()))
            }

            async fn stream_ports_notify_finished(&mut self) {
                use ::futuresdr::runtime::buffer::BufferReader as __FsdrInput;
                use ::futuresdr::runtime::buffer::BufferWriter as __FsdrOutput;
                #(#stream_inputs_notify)*
                #(#stream_outputs_notify)*
            }

            fn message_inputs() -> &'static[&'static str] {
                static MESSAGE_INPUTS: &[&str] = &[#(#message_input_names),*];
                MESSAGE_INPUTS
            }
            fn message_outputs() -> &'static[&'static str] {
                static MESSAGE_OUTPUTS: &[&str] = &[#(#message_output_names),*];
                MESSAGE_OUTPUTS
            }
            async fn call_handler(
                &mut self,
                io: &mut #work_io_type,
                mo: &mut ::futuresdr::runtime::dev::MessageOutputs,
                meta: &mut ::futuresdr::runtime::dev::BlockMeta,
                id: ::futuresdr::runtime::PortId,
                p: ::futuresdr::runtime::Pmt) ->
                    ::futuresdr::runtime::Result<::futuresdr::runtime::Pmt, ::futuresdr::runtime::Error> {
                        use ::futuresdr::runtime::BlockPortCtx;
                        use ::futuresdr::runtime::Error;
                        use ::futuresdr::runtime::Pmt;
                        use ::futuresdr::runtime::PortId;
                        use ::futuresdr::runtime::Result;
                        let ret: Result<Pmt> = match id.name() {
                                #(#handler_matches)*
                                _ => return Err(Error::InvalidMessagePort(
                                    BlockPortCtx::None,
                                    id)),
                        };

                        #[allow(unreachable_code)]
                        ret.map_err(|e| Error::HandlerError(e.to_string()))
            }
        }

        #kernel
    };
    // println!("{}", pretty_print(&expanded));
    proc_macro::TokenStream::from(expanded)
}

#[allow(dead_code)]
fn pretty_print(ts: &proc_macro2::TokenStream) -> String {
    let syntax_tree = syn::parse2(ts.clone()).unwrap();
    prettyplease::unparse(&syntax_tree)
}
