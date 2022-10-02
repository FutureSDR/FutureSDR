#![feature(stmt_expr_attributes)]
#![feature(extend_one)]
#![feature(proc_macro_internals)]
use proc_macro::TokenStream;
use quote::quote;
use quote::__private::Span;
use syn::{Block, Ident};
use syn::__private::TokenStream2;
use std::collections::HashSet;

#[proc_macro_attribute]
pub fn flowgraph(attr: TokenStream, item: TokenStream)-> TokenStream {
    let flowgraph_name: Ident = syn::parse(attr).unwrap();
    let mut r: TokenStream2 = impl_hello_macro(&flowgraph_name);
    if let Some(first_token) = item.into_iter().next() {
        let mut blocks_idents = HashSet::<Ident>::new();
        let s = first_token.to_string();
        let gen: TokenStream2 = quote! {
            println!("\ttoken {}", stringify!(#s));
        };
        r.extend_one(gen);

        // connect the ports appropriately
        let mut connexions = TokenStream2::new();
        let flowgraph: Block = syn::parse(first_token.into()).expect("valid flowgraph block");
        for stmt in flowgraph.stmts {
            if let syn::Stmt::Semi(syn::Expr::Binary(binary_expr), _) = stmt {
                // Expecting blk1[.stream1] < blk2[.stream2]
                if let Some((blk1, stream1)) = retrieve_info(*binary_expr.left) {
                    if let Some((blk2, stream2)) = retrieve_info(*binary_expr.right) {
                        blocks_idents.insert(blk1.clone());
                        blocks_idents.insert(blk2.clone());
                        let stream1 = stream1.unwrap_or(syn::Ident::new("out", Span::call_site())).to_string().replace("r#", "");
                        let stream2 = stream2.unwrap_or(syn::Ident::new_raw("in", Span::call_site())).to_string().replace("r#", "");
                        let gen: TokenStream2 = quote! {
                            println!("{}.connect_stream({},\t\"{}\",\t{},\t\"{}\");",
                                stringify!(#flowgraph_name),
                                stringify!(#blk1),
                                #stream1,
                                stringify!(#blk2),
                                #stream2
                            );
                            #flowgraph_name.connect_stream(#blk1, #stream1, #blk2, #stream2)?;
                        };
                        connexions.extend_one(gen);
                    }
                }
            }
        }

        // Add all the blocks to the `Flowgraph`...
        let mut block_insertion = TokenStream2::new();
        for blk_id in blocks_idents {
            let gen: TokenStream2 = quote! {
                let #blk_id = #flowgraph_name.add_block(#blk_id);
            };
            block_insertion.extend_one(gen);
        }
        r.extend_one(block_insertion);
        r.extend(connexions);
    }
    r.into()
}

fn exprpath_to_ident(expr: syn::ExprPath) -> Option<Ident> {
    let mut p = expr.path.segments;
    let v = p.pop();
    let v = v?.into_tuple().0.ident;
    Some(v)
}

fn expr_to_ident(expr: syn::Expr) -> Option<Ident> {
    if let syn::Expr::Path(p) = expr {
        return exprpath_to_ident(p);
    }
    None
}

fn retrieve_info(expr: syn::Expr) -> Option<(syn::Ident, Option<syn::Ident>)> {
    if let syn::Expr::Path(p) = expr {
        let v = exprpath_to_ident(p);
        return Some((v?, None));
    } else if let syn::Expr::Field(p) = expr {
        let blk = expr_to_ident(*p.base);
        let stream_id: Option<Ident>;
        if let syn::Member::Named(p) = p.member {
            stream_id = Some(p);
        } else {
            stream_id = None;
        }
        return Some((blk?, stream_id));
    }
    None
}

fn impl_hello_macro(ast: &Ident) -> TokenStream2 {
    let name = ast;
    let gen = quote! {
        println!("Hello, Macro! My name is {}!", stringify!(#name));
    };
    gen
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
