pub mod cs_tokens;

use cs_tokens::{Attr, Class, Method, NameSpace, VariableBuilder};
use quote::ToTokens;
use syn::{
    token::Enum, Attribute, FnArg, Item, ItemEnum, ItemFn, ItemStruct, Meta, Pat, ReturnType,
    Signature, Type,
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
            parse_file(file, &mut class, name_space);
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

        std::fs::write(path, scope.as_bytes()).unwrap();
    }
}

fn parse_file(file: &'static str, class: &mut Class, name_space: &mut NameSpace) {
    let parsed = syn::parse_file(file).unwrap();
    for elm in &parsed.items {
        handle_type(elm, class, name_space)
    }

    for elm in &parsed.items {
        handle_fn(elm, class, name_space)
    }
}

fn handle_type(elm: &Item, class: &mut Class, name_space: &mut NameSpace) {
    match elm {
        Item::Struct(ItemStruct {
            attrs,
            fields,
            vis,
            ident,
            ..
        }) => {
            println!("{:?}", ident);
            let mut _struct = cs_tokens::Struct::new(ident.to_string());
            for attr in attrs {
                if let Meta::List(meta) = &attr.meta {
                    let last = meta.path.segments.last().unwrap();
                    if last.ident == "repr" && meta.tokens.to_string() == "C" {
                        for field in fields {
                            let Some(name) = &field.ident else {
                                unimplemented!("Unnamed fields are not supported");
                            };
                            // TODO: need to support Repr(C) types here
                            let mut safe = true;
                            let ty = determinte_type(&field.ty, &mut safe);
                            let mut field = cs_tokens::Field::new(name.to_string())
                                .ty(ty)
                                .vis(cs_tokens::Vis::Public);

                            if !safe {
                                field.add_qualifier(cs_tokens::Qualifier::Unsafe)
                            }

                            _struct.add_field(field);
                        }
                    } else {
                    }
                } else {
                    // TODO: We should be storing a list of all supported types, and throwing an error if this isn't in the list
                }
            }

            name_space.add_struct(_struct);
        }
        Item::Enum(ItemEnum {
            attrs,
            variants,
            vis,
            ident,
            ..
        }) => {}
        _ => {}
    }
}

pub fn handle_fn(elm: &Item, class: &mut Class, name_space: &mut NameSpace) {
    match elm {
        Item::Fn(ItemFn { attrs, sig, .. }) => {
            if !attrs.is_empty() {
                for attr in attrs {
                    handle_attrs(attr, sig, class);
                }
            }
        }

        _ => {}
    }
}

fn cs_rs_supported(maybe_supported: &str) -> Option<cs_tokens::Type> {
    match maybe_supported {
        "()" => cs_tokens::Type::Void.into(),
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
        "String" => cs_tokens::Type::String.into(),
        _ => None,
    }
}

fn cs_argument(rust_arg: &FnArg, method: &mut Method, safe: &mut bool) {
    let FnArg::Typed(t) = rust_arg else {
        unimplemented!("Methods are unsupported")
    };

    let Pat::Ident(name) = t.pat.as_ref() else {
        unreachable!();
    };

    let name = &name.ident;

    let ty = determinte_type(&t.ty, safe);

    match ty {
        cs_tokens::Type::String => {
            method.arg(format!("{name}_ptr"), char_pointer());
            method.arg(format!("{name}_len"), cs_tokens::Type::Nuint);
        }
        _ => method.arg(name.to_string(), ty),
    }
}

fn char_pointer() -> cs_tokens::Type {
    cs_tokens::Type::Ptr(Box::new(cs_tokens::Type::Char))
}

fn determinte_type(ty: &Type, safe: &mut bool) -> cs_tokens::Type {
    match ty {
        Type::Ptr(ptr) => {
            *safe = false;
            let ty = determinte_type(&ptr.elem, safe);
            cs_tokens::Type::Ptr(Box::new(ty))
        }
        Type::Path(p) => {
            let type_name = p.path.segments.last().unwrap().ident.to_string();

            if let Some(supported) = cs_rs_supported(&type_name) {
                supported
            } else {
                // We should handle repr(C) types here
                cs_tokens::Type::Verbatim(type_name)
            }
        }
        _ => todo!("{ty:?}"),
    }
}

fn handle_attrs(attr: &Attribute, sig: &Signature, class: &mut Class) {
    match &attr.meta {
        Meta::Path(p) => {
            if p.segments[0].ident != "dotnetfunction" {
                return;
            }

            create_method(sig, class);
        }
        _ => {}
    }
}

fn create_method(sig: &Signature, class: &mut Class) {
    let function_name = sig.ident.to_string();

    let linkname_attr = Attr::new("DllImport".into())
        .arg("__DllName".into())
        .arg_value("EntryPoint".into(), format!("\"{function_name}\""))
        .arg_value("CallingConvention".into(), "CallingConvention.Cdecl".into())
        .arg_value("ExactSpelling".into(), "true".into());

    let mut method = Method::new(function_name)
        .vis(cs_tokens::Vis::Public)
        .attr(linkname_attr)
        .qualifier(cs_tokens::Qualifier::Static)
        .qualifier(cs_tokens::Qualifier::Extern);

    let mut safe = true;

    sig.inputs
        .iter()
        .for_each(|arg| cs_argument(arg, &mut method, &mut safe));

    let mut cs_return_type = |ret: &ReturnType| match ret {
        ReturnType::Default => cs_tokens::Type::Void,
        ReturnType::Type(_, ty) => match determinte_type(ty, &mut safe) {
            cs_tokens::Type::String => cs_tokens::Type::Verbatim("RustString".into()),
            ty => ty,
        },
    };

    method.ret(cs_return_type(&sig.output));

    if !safe {
        method.add_qualifier(cs_tokens::Qualifier::Unsafe);
    }

    class.add_method(method);
}

#[test]
fn generate() {
    let mut gen = Generator::new("csbindings");
    gen.add_file(include_str!("../../csbindings/src/lib.rs"));
    gen.add_file(include_str!("../../csbindings/src/internal/state.rs"));
    gen.add_file(include_str!("../../csbindings/src/internal/tasks.rs"));
    gen.dll_name("csbindings");
    gen.generate("NativeMethods.cs");
}
