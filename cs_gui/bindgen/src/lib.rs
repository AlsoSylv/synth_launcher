pub mod cs_tokens;

use std::io::Write;

use cs_tokens::{Attr, Class, Method, NameSpace, VariableBuilder};
use syn::{
    Attribute, FnArg, Item, ItemFn, ItemStruct, Meta, Pat, QSelf, ReturnType, Signature, Type,
};

use crate::cs_tokens::ScopeBuilder;

pub struct Generator {
    name_space: &'static str,
    files: Vec<&'static str>,
    dll_name: &'static str,
}

impl Generator {
    pub fn new(name_space: &'static str) -> Generator {
        Generator {
            name_space,
            dll_name: "",
            files: vec![],
        }
    }

    pub fn dll_name(&mut self, dll_name: &'static str) {
        self.dll_name = dll_name;
    }

    pub fn add_file(&mut self, path: &'static str) {
        self.files.push(path);
    }

    pub fn generate(&self, path: &str) {
        let mut cs_file = std::fs::File::create(path).unwrap();
        let mut scope = ScopeBuilder::new()
            .import("System.Runtime.InteropServices".into())
            .namespace(NameSpace::new(self.name_space.into()))
            .build();

        let name_space = scope.name_space();

        let mut class = Class::new("NativeMethods".into())
            .vis(cs_tokens::Vis::Public)
            .qualifier(cs_tokens::Qualifier::Static)
            .qualifier(cs_tokens::Qualifier::Partial);

        let dll_const = VariableBuilder::new("__DllName".into())
            .vis(cs_tokens::Vis::Private)
            .ty(cs_tokens::Type::String)
            .val(format!("\"{}\"", self.dll_name))
            .build();

        class.add_constant(dll_const);

        for file in &self.files {
            parse_file(file, &mut class);
        }

        let repr_field = cs_tokens::Field::new("repr".into())
            .vis(cs_tokens::Vis::Private)
            .qualifier(cs_tokens::Qualifier::Unsafe)
            .qualifier(cs_tokens::Qualifier::Fixed)
            .ty(cs_tokens::Type::FixedBuffer(
                Box::new(cs_tokens::Type::Nuint),
                3,
            ));

        let rust_string = cs_tokens::Struct::new("RustString".into()).field(repr_field);

        name_space.add_struct(rust_string);
        name_space.add_class(class);

        let scope = scope.to_string();

        println!("{}", scope);

        cs_file.write_all(scope.as_bytes()).unwrap();
    }
}

fn parse_file(file: &'static str, class: &mut Class) {
    let parsed = syn::parse_file(file).unwrap();
    for elm in parsed.items {
        handle_elm(&elm, class)
    }
}

pub fn handle_elm(elm: &Item, class: &mut Class) {
    match elm {
        Item::Fn(ItemFn { attrs, sig, .. }) => {
            if !attrs.is_empty() {
                for attr in attrs {
                    handle_attrs(attr, sig, class);
                }
            }
        }
        Item::Struct(ItemStruct {
            attrs,
            fields,
            vis,
            ident,
            ..
        }) => {
            for attr in attrs {
                if let Meta::List(meta) = &attr.meta {
                    let last = meta.path.segments.last().unwrap();
                    if last.ident == "repr" && meta.tokens.to_string() == "C" {
                        println!("{:?}", ident)
                    }
                } else {
                    // TODO: We should be storing a list of all supported types, and throwing an error if this isn't in the list
                }
            }
        }
        _ => {}
    }
}

pub fn rs_cs_primitive(maybe_prm: &str) -> Option<cs_tokens::Type> {
    match maybe_prm {
        "bool" => cs_tokens::Type::Boolean.into(),
        "i8" => cs_tokens::Type::Sbyte.into(),
        "i16" => cs_tokens::Type::Short.into(),
        "i32" => cs_tokens::Type::Int.into(),
        "i64" => cs_tokens::Type::Long.into(),
        "isize" => cs_tokens::Type::Nint.into(),
        "u8" => cs_tokens::Type::Byte.into(),
        "u16" => cs_tokens::Type::Ushort.into(),
        "u32" => cs_tokens::Type::Uint.into(),
        "u64" => cs_tokens::Type::Ulong.into(),
        "usize" => cs_tokens::Type::Nuint.into(),
        _ => None,
    }
}

pub fn rs_cs_supported(maybe_sup: &str, q_self: &Option<QSelf>) -> Option<Box<[cs_tokens::Type]>> {
    match maybe_sup {
        "String" => Some(
            [
                cs_tokens::Type::Ptr(Box::new(cs_tokens::Type::Char)),
                cs_tokens::Type::Nuint,
            ]
            .into(),
        ),
        _ => None,
    }
}

pub fn rs_supported_return(maybe_sup: &str, q_self: &Option<QSelf>) -> Option<cs_tokens::Type> {
    match maybe_sup {
        "String" => cs_tokens::Type::Verbatim("RustString".into()).into(),
        _ => None,
    }
}

fn cs_argument(rust_arg: &FnArg, method: &mut Method, safe: &mut bool) {
    if let FnArg::Typed(t) = rust_arg {
        let Pat::Ident(name) = t.pat.as_ref() else {
            unreachable!();
        };

        let name = name.ident.to_string();

        match t.ty.as_ref() {
            Type::Ptr(ptr) => {
                *safe = false;
                match ptr.elem.as_ref() {
                    Type::Path(p) => {
                        let ty = cs_tokens::Type::Ptr(
                            cs_tokens::Type::Verbatim(
                                p.path.segments.last().unwrap().ident.to_string(),
                            )
                            .into(),
                        );
                        method.arg(name, ty)
                    }
                    e => todo!("{e:?}"),
                }
            }
            Type::Path(p) => {
                let type_name = p.path.segments.last().unwrap().ident.to_string();
                // println!("{}", type_name);
                if let Some(prm) = rs_cs_primitive(&type_name) {
                    method.arg(name, prm);
                } else if let Some(maybe_sup) = rs_cs_supported(&type_name, &p.qself) {
                    match type_name.as_str() {
                        "String" => {
                            let mut maybe_sup = maybe_sup.into_vec();
                            method.arg(format!("{name}_ptr"), maybe_sup.remove(0));
                            method.arg(format!("{name}_len"), maybe_sup.remove(0));
                        }
                        _ => unimplemented!("{type_name}"),
                    }
                } else {
                    unimplemented!("We should handle repr(C) types here")
                }
            }
            _ => todo!(),
        }
    } else {
        unimplemented!("Methods are unsupported")
    }
}

pub fn handle_attrs(attr: &Attribute, sig: &Signature, class: &mut Class) {
    match &attr.meta {
        Meta::Path(p) => {
            if p.segments[0].ident == "dotnetfunction" {
                let function_name = sig.ident.to_string();

                let mut linkname_attr = Attr::new("DllImport".into());
                linkname_attr.arg(cs_tokens::AttrArg::Value("__DllName".into()));
                linkname_attr.arg(cs_tokens::AttrArg::ArgValue((
                    "EntryPoint".into(),
                    format!("\"{function_name}\""),
                )));
                linkname_attr.arg(cs_tokens::AttrArg::ArgValue((
                    "CallingConvention".into(),
                    "CallingConvention.Cdecl".into(),
                )));
                linkname_attr.arg(cs_tokens::AttrArg::ArgValue((
                    "ExactSpelling".into(),
                    "true".into(),
                )));
                let mut method = Method::new(function_name).vis(cs_tokens::Vis::Public);
                method.attr(linkname_attr);
                method.qualifier(cs_tokens::Qualifier::Static);
                method.qualifier(cs_tokens::Qualifier::Extern);

                let mut safe = true;
                let mut safe_args = true;

                let mut cs_type = |ty: &Type| {
                    match ty {
                        Type::Ptr(ptr) => {
                            safe = false;
                            match ptr.elem.as_ref() {
                                Type::Path(p) => {
                                    let ty = p.path.segments.last().unwrap().ident.to_string();
                                    cs_tokens::Type::Ptr(Box::new(cs_tokens::Type::Verbatim(ty)))
                                }
                                e => todo!("{e:?}"),
                            }
                        }
                        Type::Path(p) => {
                            let type_name = p.path.segments.last().unwrap().ident.to_string();
                            // println!("{}", type_name);
                            if let Some(prm) = rs_cs_primitive(&type_name) {
                                prm
                            } else if let Some(ty) = rs_supported_return(&type_name, &p.qself) {
                                ty
                            } else {
                                // TODO: Check that this type is repr(C)
                                cs_tokens::Type::Verbatim(type_name)
                            }
                        }
                        _ => todo!(),
                    }
                };

                let cs_arg = |f: &FnArg| {
                    match f {
                        FnArg::Receiver(_) => unimplemented!("Methods are unsupported"),
                        FnArg::Typed(t) => {
                            let Pat::Ident(name) = t.pat.as_ref() else {
                                unreachable!();
                            };

                            let name = name.ident.to_string();

                            match t.ty.as_ref() {
                                Type::Ptr(ptr) => {
                                    safe_args = false;
                                    match ptr.elem.as_ref() {
                                        Type::Path(p) => {
                                            let ty = cs_tokens::Type::Ptr(
                                                cs_tokens::Type::Verbatim(
                                                    p.path
                                                        .segments
                                                        .last()
                                                        .unwrap()
                                                        .ident
                                                        .to_string(),
                                                )
                                                .into(),
                                            );
                                            method.arg(name, ty)
                                        }
                                        e => todo!("{e:?}"),
                                    }
                                }
                                Type::Path(p) => {
                                    let type_name =
                                        p.path.segments.last().unwrap().ident.to_string();
                                    // println!("{}", type_name);
                                    if let Some(prm) = rs_cs_primitive(&type_name) {
                                        method.arg(name, prm);
                                    } else if let Some(maybe_sup) =
                                        rs_cs_supported(&type_name, &p.qself)
                                    {
                                        match type_name.as_str() {
                                            "String" => {
                                                let mut maybe_sup = maybe_sup.into_vec();
                                                method.arg(
                                                    format!("{name}_ptr"),
                                                    maybe_sup.remove(0),
                                                );
                                                method.arg(
                                                    format!("{name}_len"),
                                                    maybe_sup.remove(0),
                                                );
                                            }
                                            _ => unimplemented!("{type_name}"),
                                        }
                                    } else {
                                        // TODO: Check that this type is repr(C)
                                        method.arg(name, cs_tokens::Type::Verbatim(type_name));
                                    }
                                }
                                _ => todo!(),
                            }
                        }
                    }
                };

                sig.inputs.iter().for_each(cs_arg);

                let mut cs_return_type = |ret: &ReturnType| match ret {
                    ReturnType::Default => cs_tokens::Type::Void,
                    ReturnType::Type(_, ty) => cs_type(ty),
                };

                method.ret(cs_return_type(&sig.output));

                if !safe || !safe_args {
                    method.qualifier(cs_tokens::Qualifier::Unsafe);
                }

                class.add_method(method);
            }
        }
        _ => {}
    }
}

#[test]
fn generate() {
    let mut gen = Generator::new("csbindings");
    gen.add_file(include_str!("../../csbindings/src/lib.rs"));
    gen.dll_name("csbindings");
    gen.generate("NativeMethods.cs");
}
