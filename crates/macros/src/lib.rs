//! Macros to make working with FutureSDR a bit nicer.
use proc_macro::TokenStream;
use quote::quote;
use syn::bracketed;
use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::parse_macro_input;
use syn::punctuated::Punctuated;
use syn::token;
use syn::Attribute;
use syn::Data;
use syn::DeriveInput;
use syn::Fields;
use syn::GenericParam;
use syn::Ident;
use syn::Index;
use syn::Meta;
use syn::PathArguments;
use syn::Result;
use syn::Token;
use syn::Type;

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
/// let src = fg.add_block(src)?;
/// let shift = fg.add_block(shift)?;
/// let resamp1 = fg.add_block(resamp1)?;
/// let demod = fg.add_block(demod)?;
/// let resamp2 = fg.add_block(resamp2)?;
/// let snk = fg.add_block(snk)?;
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
#[proc_macro]
pub fn connect(input: TokenStream) -> TokenStream {
    let connect_input = parse_macro_input!(input as ConnectInput);
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
                ConnectionType::Stream => {
                    let src_port = match src_port {
                        Some(Port { name, index: None }) => {
                            quote! { #name() }
                        }
                        Some(Port {
                            name,
                            index: Some(i),
                        }) => {
                            quote! { #name().get_mut(#i).unwrap() }
                        }
                        None => {
                            quote!(output())
                        }
                    };
                    let dst_port = match &dst.input {
                        Some(Port { name, index: None }) => {
                            quote! { #name() }
                        }
                        Some(Port {
                            name,
                            index: Some(i),
                        }) => {
                            quote! { #name().get_mut(#i).unwrap() }
                        }
                        None => {
                            quote!(input())
                        }
                    };
                    let dst_block = &dst.block;
                    quote! {
                        #fg.connect_stream(#src_block.get().#src_port, #dst_block.get().#dst_port);
                    }
                }
                ConnectionType::Circuit => {
                    let src_port = match src_port {
                        Some(Port { name, index: None }) => {
                            quote! { #name() }
                        }
                        Some(Port {
                            name,
                            index: Some(i),
                        }) => {
                            quote! { #name().get_mut(#i).unwrap() }
                        }
                        None => {
                            quote!(output())
                        }
                    };
                    let dst_port = match &dst.input {
                        Some(Port { name, index: None }) => {
                            quote! { #name() }
                        }
                        Some(Port {
                            name,
                            index: Some(i),
                        }) => {
                            quote! { #name().get_mut(#i).unwrap() }
                        }
                        None => {
                            quote!(input())
                        }
                    };
                    let dst_block = &dst.block;
                    quote! {
                        #src_block.get().#src_port.close_circuit(#dst_block.get().#dst_port);
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
                        #fg.connect_message(&#src_block, #src_port, &#dest_block, #dst_port)?;
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

    // Generate block declarations
    let block_decls = blocks.iter().map(|block| {
        quote! {
            let #block = #fg.add(#block);
        }
    });

    let out = quote! {
        use futuresdr::runtime::BlockId;
        use futuresdr::runtime::BlockRef;
        use futuresdr::runtime::Flowgraph;
        use futuresdr::runtime::Kernel;
        use futuresdr::runtime::KernelInterface;
        use std::result::Result;

        pub trait AddToFg<K: Kernel + KernelInterface + 'static> {
            fn add_to_fg(self, fg: &mut Flowgraph) -> BlockRef<K>;
        }
        impl<K: Kernel + KernelInterface + 'static> AddToFg<K> for K {
            fn add_to_fg(self, fg: &mut Flowgraph) -> BlockRef<K> {
                fg.add_block(self)
            }
        }
        impl<K: Kernel + KernelInterface + 'static> AddToFg<K> for BlockRef<K> {
            fn add_to_fg(self, _fg: &mut Flowgraph) -> BlockRef<K> {
                self
            }
        }
        pub trait FgOps {
            fn add<T, K>(&mut self, item: T) -> BlockRef<K>
            where
                T: AddToFg<K>,
                K: Kernel + KernelInterface + 'static;
        }
        impl FgOps for Flowgraph {
            fn add<T, K>(&mut self, item: T) -> BlockRef<K>
            where
                T: AddToFg<K>,
                K: Kernel + KernelInterface + 'static,
            {
                item.add_to_fg(self)
            }
        }

        #(#block_decls)*
        #(#connections)*
        (#(#blocks),*)
    };

    let out = quote![
        #[allow(unused_variables)]
        let (#(#blocks),*) = {
            #out
        };
    ];

    // let tmp = quote!(fn foo() { #out });
    // println!("{}", pretty_print(&tmp));
    // println!("{}", &out);
    out.into()
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

        while let Ok(ct) = input.parse::<ConnectionType>() {
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
    Message,
    Circuit,
}

impl Parse for ConnectionType {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(Token![>]) {
            input.parse::<Token![>]>()?;
            Ok(Self::Stream)
        } else if input.peek(Token![|]) {
            input.parse::<Token![|]>()?;
            Ok(Self::Message)
        } else if input.peek(Token![<]) {
            input.parse::<Token![<]>()?;
            Ok(Self::Circuit)
        } else {
            Err(input.error("expected `>` or `|` to specify the connection type"))
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

//=========================================================================
// BLOCK MACRO
//=========================================================================
/// Block Macro
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

    let unconstraint_params = generics.params.iter().map(|param| match param {
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
    });

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
                                    names.push(format!("{}{}", #field_name_str, i));
                                }
                            };
                            let init_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    self.#field_name[i].init(block_id, PortId(format!("{}{}", #field_name_str, i)), inbox.clone());
                                }
                            };
                            let validate_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    self.#field_name[i].validate()?;
                                }
                            };
                            let notify_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    self.#field_name[i].notify_finished().await;
                                }
                            };
                            let finish_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if port == format!("{}{}", #field_name_str, i) {
                                        self.#field_name[i].finish();
                                        return Ok(());
                                    }
                                }
                            };
                            let get_input_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if name == format!("{}{}", #field_name_str, i) {
                                        Some(&mut self.#field_name[i]);
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
                                    names.push(format!("{}{}", #field_name_str, i));
                                }
                            };
                            let init_code = quote! {
                                for i in 0..#len {
                                    self.#field_name[i].init(block_id, PortId(format!("{}{}", #field_name_str, i)), inbox.clone());
                                }
                            };
                            let validate_code = quote! {
                                for i in 0..#len {
                                    self.#field_name[i].validate()?;
                                }
                            };
                            let notify_code = quote! {
                                for i in 0..#len {
                                    self.#field_name[i].notify_finished().await;
                                }
                            };
                            let finish_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if port == format!("{}{}", #field_name_str, i) {
                                        self.#field_name[i].finish();
                                        return Ok(());
                                    }
                                }
                            };
                            let get_input_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if name == format!("{}{}", #field_name_str, i) {
                                        return Some(&mut self.#field_name[i]);
                                    }
                                }
                            };
                            Some((name_code, init_code, validate_code, notify_code, finish_code, get_input_code))
                        }
                        // Handle tuples (T1, T2, ...)
                        Type::Tuple(tuple) => {
                            let len = tuple.elems.len();
                            let name_code = quote! {
                                for i in 0..#len {
                                    names.push(format!("{}{}", #field_name_str, i));
                                }
                            };
                            let init_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    self.#field_name.#index.init(block_id, PortId(format!("{}{}", #field_name_str, #index)), inbox.clone());
                                }
                            });
                            let init_code = quote! {
                                #(#init_code)*
                            };
                            let validate_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    self.#field_name.#index.validate()?;
                                }
                            });
                            let validate_code = quote! {
                                #(#validate_code)*
                            };
                            let notify_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    self.#field_name.#index.notify_finished().await;
                                }
                            });
                            let notify_code = quote! {
                                #(#notify_code)*
                            };
                            let finish_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote!{
                                    if port == format!("{}{}", #field_name_str, #index) {
                                        self.#field_name.#index.finish();
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
                                    if name == format!("{}{}", #field_name_str, #index) {
                                        return Some(&mut self.#field_name.#index);
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
                                names.push(#field_name_str.to_string());
                            };
                            let init_code = quote! {
                                self.#field_name.init(block_id, PortId(#field_name_str.to_string()), inbox.clone());
                            };
                            let validate_code = quote! {
                                self.#field_name.validate()?;
                            };
                            let notify_code = quote! {
                                self.#field_name.notify_finished().await;
                            };
                            let finish_code = quote! {
                                if port == #field_name_str {
                                    self.#field_name.finish();
                                    return Ok(());
                                }
                            };
                            let get_input_code = quote! {
                                if name == #field_name_str {
                                    return Some(&mut self.#field_name)
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

    let stream_inputs_names = stream_inputs
        .iter()
        .map(|x| x.0.clone())
        .collect::<Vec<_>>();
    let stream_inputs_init = stream_inputs
        .iter()
        .map(|x| x.1.clone())
        .collect::<Vec<_>>();
    let stream_inputs_validate = stream_inputs
        .iter()
        .map(|x| x.2.clone())
        .collect::<Vec<_>>();
    let stream_inputs_notify = stream_inputs
        .iter()
        .map(|x| x.3.clone())
        .collect::<Vec<_>>();
    let stream_inputs_finish = stream_inputs
        .iter()
        .map(|x| x.4.clone())
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
                                    names.push(format!("{}{}", #field_name_str, i));
                                }
                            };
                            let init_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    self.#field_name[i].init(block_id, PortId(format!("{}{}", #field_name_str, i)), inbox.clone());
                                }
                            };
                            let validate_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    self.#field_name[i].validate()?;
                                }
                            };
                            let notify_code = quote! {
                                for i in 0..self.#field_name.len() {
                                    self.#field_name[i].notify_finished().await;
                                }
                            };
                            let connect_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if name == format!("{}{}", #field_name_str, i) {
                                        return self.#field_name[i].connect_dyn(reader);
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
                                    names.push(format!("{}{}", #field_name_str, i));
                                }
                            };
                            let init_code = quote! {
                                for i in 0..#len {
                                    self.#field_name[i].init(block_id, PortId(format!("{}{}", #field_name_str, i)), inbox.clone());
                                }
                            };
                            let validate_code = quote! {
                                for i in 0..#len {
                                    self.#field_name[i].validate()?;
                                }
                            };
                            let notify_code = quote! {
                                for i in 0..#len {
                                    self.#field_name[i].notify_finished().await;
                                }
                            };
                            let connect_code = quote! {
                                for (i, _) in self.#field_name.iter_mut().enumerate() {
                                    if name == format!("{}{}", #field_name_str, i) {
                                        return self.#field_name[i].connect_dyn(reader);
                                    }
                                }
                            };
                            Some((name_code, init_code, validate_code, notify_code, connect_code))
                        }
                        // Handle tuples (T1, T2, ...)
                        Type::Tuple(tuple) => {
                            let len = tuple.elems.len();
                            let name_code = quote! {
                                for i in 0..#len {
                                    names.push(format!("{}{}", #field_name_str, i));
                                }
                            };
                            let init_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    self.#field_name.#index.init(block_id, PortId(format!("{}{}", #field_name_str, #index)), inbox.clone());
                                }
                            });
                            let init_code = quote! {
                                #(#init_code)*
                            };
                            let validate_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    self.#field_name.#index.validate()?;
                                }
                            });
                            let validate_code = quote! {
                                #(#validate_code)*
                            };
                            let notify_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote! {
                                    self.#field_name.#index.notify_finished().await;
                                }
                            });
                            let notify_code = quote! {
                                #(#notify_code)*
                            };
                            let connect_code = tuple.elems.iter().enumerate().map(|(i, _)| {
                                let index = syn::Index::from(i);
                                quote!{
                                    if name == format!("{}{}", #field_name_str, #index) {
                                        return self.#field_name.#index.connect_dyn(reader);
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
                                names.push(#field_name_str.to_string());
                            };
                            let init_code = quote! {
                                self.#field_name.init(block_id, PortId(#field_name_str.to_string()), inbox.clone());
                            };
                            let validate_code = quote! {
                                self.#field_name.validate()?;
                            };
                            let notify_code = quote! {
                                self.#field_name.notify_finished().await;
                            };
                            let connect_code = quote! {
                                if name == #field_name_str {
                                    return self.#field_name.connect_dyn(reader);
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

    let stream_outputs_names = stream_outputs
        .iter()
        .map(|x| x.0.clone())
        .collect::<Vec<_>>();
    let stream_outputs_init = stream_outputs
        .iter()
        .map(|x| x.1.clone())
        .collect::<Vec<_>>();
    let stream_outputs_validate = stream_outputs
        .iter()
        .map(|x| x.2.clone())
        .collect::<Vec<_>>();
    let stream_outputs_notify = stream_outputs
        .iter()
        .map(|x| x.3.clone())
        .collect::<Vec<_>>();
    let stream_outputs_connect = stream_outputs
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
                            panic!("message handlers have to be an identifier or identifier = \"port name\"");
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
            kernel = quote! {
                #[doc(hidden)]
                impl #generics ::futuresdr::runtime::Kernel for #struct_name #generics
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
    let message_input_names = message_input_names.into_iter().map(|handler| {
        let handler = if let Some(stripped) = handler.strip_prefix("r#") {
            stripped.to_string()
        } else {
            handler
        };
        quote! {
            #handler
        }
    });

    // Generate match arms for the handle method
    let handler_matches =
        message_inputs
            .iter()
            .zip(message_input_names.clone())
            .map(|(handler, handler_name)| {
                quote! {
                    #handler_name  => self.#handler(io, mio, meta, p).await,
                }
            });

    let expanded = quote! {

        impl #generics #struct_name #unconstraint_generics
            #where_clause
        {
            #(#port_getter_fns)*
        }

        impl #generics ::futuresdr::runtime::KernelInterface for #struct_name #unconstraint_generics
            #where_clause
        {
            fn is_blocking() -> bool {
                #blocking
            }
            fn type_name() -> &'static str {
                static TYPE_NAME: &str = #type_name;
                TYPE_NAME
            }
            fn stream_inputs(&self) -> Vec<String> {
                let mut names = vec![];
                #(#stream_inputs_names)*
                names
            }
            fn stream_outputs(&self) -> Vec<String> {
                let mut names = vec![];
                #(#stream_outputs_names)*
                names
            }

            fn stream_ports_init(&mut self, block_id: ::futuresdr::runtime::BlockId, inbox: ::futuresdr::channel::mpsc::Sender<::futuresdr::runtime::BlockMessage>) {
                use ::futuresdr::runtime::PortId;
                #(#stream_inputs_init)*
                #(#stream_outputs_init)*
            }
            fn stream_ports_validate(&self) -> ::futuresdr::runtime::Result<(), ::futuresdr::runtime::Error> {
                use ::futuresdr::runtime::PortId;
                #(#stream_inputs_validate)*
                #(#stream_outputs_validate)*
                Ok(())
            }
            fn stream_input_finish(&mut self, port_id: ::futuresdr::runtime::PortId) -> ::futuresdr::runtime::Result<(), futuresdr::runtime::Error> {
                use ::futuresdr::runtime::Error;
                use ::futuresdr::runtime::BlockPortCtx;
                let port = port_id.0.as_str();
                #(#stream_inputs_finish)*
                Err(Error::InvalidMessagePort(BlockPortCtx::None, port_id))
            }
            async fn stream_ports_notify_finished(&mut self) {
                #(#stream_inputs_notify)*
                #(#stream_outputs_notify)*
            }
            fn stream_input(&mut self, name: &str) -> Option<&mut dyn ::futuresdr::runtime::buffer::BufferReader> {
                #(#stream_inputs_get)*
                None
            }
            fn connect_stream_output(&mut self, name: &str, reader: &mut dyn ::futuresdr::runtime::buffer::BufferReader) -> ::futuresdr::runtime::Result<(), ::futuresdr::runtime::Error> {
                use ::futuresdr::runtime::Error;
                use ::futuresdr::runtime::BlockPortCtx;
                #(#stream_outputs_connect)*
                Err(Error::InvalidStreamPort(BlockPortCtx::None, name.into()))
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
                io: &mut ::futuresdr::runtime::WorkIo,
                mio: &mut ::futuresdr::runtime::MessageOutputs,
                meta: &mut ::futuresdr::runtime::BlockMeta,
                id: ::futuresdr::runtime::PortId,
                p: ::futuresdr::runtime::Pmt) ->
                    ::futuresdr::runtime::Result<::futuresdr::runtime::Pmt, ::futuresdr::runtime::Error> {
                        use ::futuresdr::runtime::BlockPortCtx;
                        use ::futuresdr::runtime::Error;
                        use ::futuresdr::runtime::Pmt;
                        use ::futuresdr::runtime::PortId;
                        use ::futuresdr::runtime::Result;
                        let ret: Result<Pmt> = match id.0.as_str() {
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

//=========================================================================
// ASYNC_TRAIT
//=========================================================================

/// Custom version of async_trait that uses non-send futures for WASM.
#[proc_macro_attribute]
pub fn async_trait(
    _attr: proc_macro::TokenStream,
    fun: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let fun: proc_macro2::TokenStream = fun.into();
    quote!(
        #[cfg_attr(not(target_arch = "wasm32"), futuresdr::macros::async_trait_orig)]
        #[cfg_attr(target_arch = "wasm32", futuresdr::macros::async_trait_orig(?Send))]
        #fun
    )
    .into()
}
