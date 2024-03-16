pub mod cs_tokens;

use std::{fs::File, io::Write};

use cs_tokens::{Class, NameSpace, VariableBuilder};
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
            .qualifier(cs_tokens::Qualifier::ReadOnly)
            .ty(cs_tokens::Type::String)
            .val(format!("\"{}\"", self.dll_name))
            .build();

        class.add_constant(dll_const);

        name_space.add_class(class);

        println!("{}", scope.to_string());

        cs_file
            .write_all(format!("namespace {} {{\n", self.name_space).as_bytes())
            .unwrap();
        cs_file
            .write_all("\tpublic static partial class NativeMethods {\n".as_bytes())
            .unwrap();
        cs_file
            .write_all(
                format!(
                    "\t\tprivate const string __DllName = \"{}\";\n\n",
                    self.dll_name
                )
                .as_bytes(),
            )
            .unwrap();
        for file in &self.files {
            parse_file(file, &mut cs_file);
        }

        cs_file
            .write_all(b"\t\tpublic struct RustString { private unsafe fixed nuint repr[3]; }\n")
            .unwrap();
        cs_file.write(b"\t}\n").unwrap();
        cs_file.write(b"}\n").unwrap();
    }
}

fn parse_file(file: &'static str, cs_file: &mut File) {
    let parsed = syn::parse_file(file).unwrap();
    for elm in parsed.items {
        handle_elm(&elm, cs_file)
    }
}

pub fn handle_elm(elm: &Item, cs_file: &mut File) {
    match elm {
        Item::Fn(ItemFn { attrs, sig, .. }) => {
            if !attrs.is_empty() {
                for attr in attrs {
                    handle_attrs(attr, sig, cs_file);
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
                    if last.ident.to_string() == "repr" {
                        if meta.tokens.to_string() == "C" {
                            println!("{:?}", ident)
                        }
                    }
                } else {
                }
            }
        }
        _ => {}
    }
}

pub fn rs_cs_primitive(maybe_prm: &str) -> Option<&str> {
    match maybe_prm {
        "bool" => "bool".into(),
        "i8" => "sbyte".into(),
        "i16" => "short".into(),
        "i32" => "int".into(),
        "i64" => "long".into(),
        "isize" => "nint".into(),
        "u8" => "byte".into(),
        "u16" => "ushort".into(),
        "u32" => "uint".into(),
        "u64" => "ulong".into(),
        "usize" => "nuint".into(),
        _ => None,
    }
}

pub fn rs_cs_supported(maybe_sup: &str, q_self: &Option<QSelf>) -> Option<Box<[&'static str]>> {
    match maybe_sup {
        "String" => Some(["char*", "nuint"].into()),
        _ => None,
    }
}

pub fn handle_attrs(attr: &Attribute, sig: &Signature, cs_file: &mut File) {
    match &attr.meta {
        Meta::Path(p) => {
            if p.segments[0].ident.to_string() == "dotnetfunction" {
                let mut cs_args = Vec::new();
                let cs_type = |ty: &Type| {
                    match ty {
                        Type::Ptr(ptr) => match ptr.elem.as_ref() {
                            Type::Path(p) => {
                                format!("{}*", p.path.segments.last().unwrap().ident.to_string())
                            }
                            e => todo!("{e:?}"),
                        },
                        Type::Path(p) => {
                            let type_name = p.path.segments.last().unwrap().ident.to_string();
                            // println!("{}", type_name);
                            if let Some(prm) = rs_cs_primitive(&type_name) {
                                prm.to_string()
                            } else {
                                if let Some(maybe_sup) = rs_cs_supported(&type_name, &p.qself) {}

                                "RustString".into()
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
                                Type::Ptr(ptr) => match ptr.elem.as_ref() {
                                    Type::Path(p) => format!(
                                        "{}* {name}",
                                        p.path.segments.last().unwrap().ident.to_string()
                                    ),
                                    e => todo!("{e:?}"),
                                },
                                Type::Path(p) => {
                                    let type_name =
                                        p.path.segments.last().unwrap().ident.to_string();
                                    // println!("{}", type_name);
                                    if let Some(prm) = rs_cs_primitive(&type_name) {
                                        prm.to_string()
                                    } else if let Some(maybe_sup) =
                                        rs_cs_supported(&type_name, &p.qself)
                                    {
                                        match type_name.as_str() {
                                            "String" => {
                                                let first = format!("{} {name}_ptr", maybe_sup[0]);
                                                let second = format!("{} {name}_len", maybe_sup[1]);
                                                format!("{first}, {second}")
                                            }
                                            _ => unimplemented!("{type_name}"),
                                        }
                                    } else {
                                        todo!()
                                    }
                                }
                                _ => todo!(),
                            }
                        }
                    }
                };

                sig.inputs
                    .iter()
                    .for_each(|input| cs_args.push(cs_arg(&input)));

                let cs_return_type = |ret: &ReturnType| match ret {
                    ReturnType::Default => "void".into(),
                    ReturnType::Type(_, ty) => cs_type(ty),
                };

                let args_str = cs_args.join(", ");

                cs_file.write_all(format!("\t\t[DllImport(__DllName, EntryPoint = \"{}\", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]\n", sig.ident.to_string()).as_bytes()).unwrap();
                cs_file
                    .write_all(
                        format!(
                            "\t\tpublic static extern unsafe {} {}({});\n\n",
                            cs_return_type(&sig.output),
                            sig.ident.to_string(),
                            args_str
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                // println!("{:?}", sig.inputs);
                // println!("{:?}", sig.output);
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
