use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use std::str::FromStr;
use syn::punctuated::Punctuated;
use syn::token::{Colon, Comma, Mut, Star};
use syn::{
    parse_macro_input, Expr, FnArg, Ident, Item, ItemFn, ItemStruct, Local, LocalInit, Pat,
    PatIdent, PatType, ReturnType, Stmt, Type, TypePtr,
};

#[proc_macro_attribute]
pub fn dotnetstruct(
    _: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let struc = parse_macro_input!(item as ItemStruct);
    let fields = &struc.fields;

    quote!(#struc).into()
}

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
        #[allow(improper_ctypes)]
        #[allow(improper_ctypes_definitions)]
        #(#attrs)*
        #vis #safety extern "C" fn #name(#new_inputs) #output {
            #(#strings)*
            #block
        }
    };

    proc_macro::TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn dotnet(
    _args: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let item = syn::parse_macro_input!(item as Item);
    match item {
        Item::Enum(_en) => {
            todo!()
        }
        Item::Struct(_struc) => {
            todo!()
        }
        Item::Fn(func) => {
            let sig = &func.sig;
            let mut new_args: Punctuated<FnArg, Comma> = Punctuated::new();
            let args = arguments(&sig.inputs, &mut new_args);
            let rust_func = &sig.ident;
            let raw_c_func = quote::format_ident!("raw_{}", rust_func);
            let output_binding = match &sig.output {
                ReturnType::Type(_, ty) => {
                    new_args.push(FnArg::Typed(PatType {
                        attrs: vec![],
                        colon_token: Colon(Span::call_site()),
                        pat: Box::new(Pat::Ident(PatIdent {
                            attrs: vec![],
                            ident: Ident::new("_return", Span::call_site()),
                            subpat: None,
                            by_ref: None,
                            mutability: None,
                        })),
                        ty: Box::new(Type::Ptr(TypePtr {
                            const_token: None,
                            elem: ty.to_owned(),
                            star_token: Star(Span::call_site()),
                            mutability: Some(Mut(Span::call_site())),
                        })),
                    }));
                    quote! { *_return =  }
                }
                ReturnType::Default => {
                    quote! {}
                }
            };
            // TODO: Transform args
            // TODO: seperate out common logic
            // TODO: Determine if type is FFI safe, and then do things correctly
            let output = quote! {
                #[no_mangle]
                pub unsafe extern fn #raw_c_func(#new_args) {
                    let raw_path = std::mem::take((*raw_path).as_mut_string());
                    #output_binding #rust_func(#args);
                }

                #func
            };
            output.into()
        }
        _ => unimplemented!(),
    }
}

fn arguments<'a>(
    inputs: &'a Punctuated<FnArg, Comma>,
    new_inputs: &mut Punctuated<FnArg, Comma>,
) -> Punctuated<&'a proc_macro2::Ident, Comma> {
    let mut args = Punctuated::new();
    for input in inputs {
        let FnArg::Typed(input) = &input else {
            unimplemented!("Methods are not supported!")
        };
        let Pat::Ident(w) = input.pat.as_ref() else {
            unreachable!()
        };

        let mut new_input = input.clone();

        match new_input.ty.as_ref() {
            Type::Path(ty) => {
                let last = ty.path.segments.last().unwrap();
                if last.ident.to_string() == "String" {
                    *new_input.ty.as_mut() = Type::Ptr(TypePtr {
                        star_token: Star(Span::call_site()),
                        const_token: None,
                        mutability: Some(Mut(Span::call_site())),
                        elem: Box::new(Type::Verbatim(
                            TokenStream::from_str("RustString").unwrap(),
                        )),
                    })
                }
            }
            _ => {}
        }

        new_inputs.push(FnArg::Typed(new_input));

        args.push(&w.ident);
    }
    args
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
