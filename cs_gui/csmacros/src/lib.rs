use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use std::str::FromStr;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{
    parse_macro_input, Expr, FnArg, ItemFn, Local, LocalInit, Pat, PatType, Stmt, Type, TypePtr,
};

#[proc_macro_attribute]
pub fn dotnetfunction(
    _: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let fun = parse_macro_input!(item as ItemFn);
    let inputs = &fun.sig.inputs;
    let mut strings = vec![];
    let mut new_inputs: Punctuated<FnArg, Comma> = Punctuated::new();
    inputs.iter().for_each(|fn_arg| match fn_arg {
        FnArg::Receiver(_) => unreachable!(),
        FnArg::Typed(t) => {
            if let Some(line) = handle_input_types(t, &mut new_inputs) {
                strings.push(line);
            } else {
                new_inputs.push(fn_arg.clone())
            }
        }
    });

    let name = &fun.sig.ident;
    let output = &fun.sig.output;
    let block = &fun.block;
    let vis = &fun.vis;
    let attrs = &fun.attrs;
    let safety = &fun.sig.unsafety;

    let expanded = quote! {
        #[no_mangle]
        #(#attrs)*
        #vis #safety extern "C" fn #name(#new_inputs) #output {
            #(#strings)*
            #block
        }
    };

    proc_macro::TokenStream::from(expanded)
}

fn handle_input_types(t: &PatType, inputs: &mut Punctuated<FnArg, Comma>) -> Option<Stmt> {
    match t.ty.as_ref() {
        Type::Path(ty) => {
            if let Some(path) = ty.path.segments.last() {
                match &path.ident.to_string()[..] {
                    "String" => {
                        let name = t.pat.to_token_stream().to_string();
                        let arg = FnArg::Typed(PatType {
                            attrs: vec![],
                            pat: Box::new(Pat::Verbatim(
                                TokenStream::from_str(&format!("{name}_ptr")).unwrap(),
                            )),
                            ty: Box::new(Type::Ptr(TypePtr {
                                star_token: Default::default(),
                                const_token: Some(Default::default()),
                                mutability: None,
                                elem: Box::new(Type::Verbatim(
                                    TokenStream::from_str("u16").unwrap(),
                                )),
                            })),
                            colon_token: Default::default(),
                        });
                        let len = FnArg::Typed(PatType {
                            attrs: vec![],
                            pat: Box::new(Pat::Verbatim(
                                TokenStream::from_str(&format!("{name}_len")).unwrap(),
                            )),
                            ty: Box::new(Type::Verbatim(TokenStream::from_str("usize").unwrap())),
                            colon_token: Default::default(),
                        });
                        inputs.push(arg);
                        inputs.push(len);
                        let line = Stmt::Local(Local {
                            attrs: vec![],
                            let_token: Default::default(),
                            pat: Pat::Verbatim(TokenStream::from_str(&name).unwrap()),
                            init: Some(LocalInit {
                                eq_token: Default::default(),
                                expr: Box::new(Expr::Verbatim(TokenStream::from_str(&format!("String::from_utf16(std::slice::from_raw_parts({name}_ptr, {name}_len))")).unwrap())),
                                diverge: None,
                            }),
                            semi_token: Default::default(),
                        });
                        Some(line)
                    }
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}
