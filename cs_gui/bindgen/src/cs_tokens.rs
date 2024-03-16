pub struct Scope {
    imports: Vec<String>,
    name_space: NameSpace,
}

impl Scope {
    pub fn name_space(&mut self) -> &mut NameSpace {
        &mut self.name_space
    }
}

pub struct ScopeBuilder {
    imports: Vec<String>,
    name_space: Option<NameSpace>,
}

impl ScopeBuilder {
    pub fn new() -> Self {
        Self {
            imports: vec![],
            name_space: None,
        }
    }

    pub fn import(mut self, path: String) -> Self {
        self.imports.push(path);

        self
    }

    pub fn namespace(mut self, name_space: NameSpace) -> Self {
        self.name_space = Some(name_space);

        self
    }

    pub fn build(self) -> Scope {
        Scope {
            imports: self.imports,
            name_space: self.name_space.unwrap(),
        }
    }
}

impl ToString for Scope {
    fn to_string(&self) -> String {
        let imports = self.imports.iter().fold(String::new(), |acc, import| {
            format!("{acc}using {import}\n")
        });

        let name_space = format!("namespace {};\n", self.name_space.name);

        let classes = self
            .name_space
            .classes
            .iter()
            .fold(String::new(), |acc, class| {
                format!("{acc}\n{}", class.to_string())
            });

        format!("{imports}\n{name_space}{classes}")
    }
}

pub struct NameSpace {
    name: String,
    classes: Vec<Class>,
}

impl NameSpace {
    pub fn new(name: String) -> Self {
        Self {
            name,
            classes: vec![],
        }
    }

    pub fn add_class(&mut self, class: Class) -> &mut Class {
        self.classes.push(class);

        let len = self.classes.len() - 1;

        &mut self.classes[len]
    }
}

pub enum Vis {
    Public,
    Internal,
    Private,
    Protected,
}

impl Vis {
    fn as_str(&self) -> &str {
        match self {
            Vis::Public => "public",
            Vis::Internal => "internal",
            Vis::Private => "private",
            Vis::Protected => "protected",
        }
    }
}

pub enum Qualifier {
    ReadOnly,
    Override,
    Static,
    Partial,
    Unsafe,
    Fixed,
    Virtual,
    Extern,
}

impl Qualifier {
    fn as_str(&self) -> &str {
        match self {
            Qualifier::ReadOnly => "readonly",
            Qualifier::Override => "override",
            Qualifier::Static => "static",
            Qualifier::Partial => "partial",
            Qualifier::Unsafe => "unsafe",
            Qualifier::Fixed => "fixed",
            Qualifier::Virtual => "virtual",
            Qualifier::Extern => "extern",
        }
    }
}

pub enum Type {
    /// Utf-16 character
    Char,
    Byte,
    Ushort,
    Uint,
    Ulong,
    Nuint,
    Sbyte,
    Short,
    Int,
    Long,
    Nint,
    Void,
    String,
    Array(Box<Type>),
    Ptr(Box<Type>),
}

impl ToString for Type {
    fn to_string(&self) -> String {
        match self {
            Type::Char => "char".into(),
            Type::Byte => todo!(),
            Type::Ushort => todo!(),
            Type::Uint => todo!(),
            Type::Ulong => todo!(),
            Type::Nuint => "nuint".to_string(),
            Type::Sbyte => todo!(),
            Type::Short => todo!(),
            Type::Int => todo!(),
            Type::Long => todo!(),
            Type::Nint => todo!(),
            Type::String => "string".into(),
            Type::Void => "void".into(),
            Type::Array(ty) => format!("{}[]", ty.to_string()),
            Type::Ptr(ty) => format!("{}*", ty.to_string()),
        }
    }
}

pub struct Class {
    constants: Vec<Variable>,
    vis: Option<Vis>,
    qualifiers: Vec<Qualifier>,
    methods: Vec<Method>,
    name: String,
}

impl Class {
    pub fn new(name: String) -> Self {
        Self {
            constants: vec![],
            vis: None,
            qualifiers: vec![],
            methods: vec![],
            name,
        }
    }

    pub fn vis(mut self, vis: Vis) -> Self {
        self.vis = Some(vis);
        self
    }

    pub fn add_constant(&mut self, var: Variable) {
        self.constants.push(var);
    }

    pub fn qualifier(mut self, qualifier: Qualifier) -> Self {
        self.qualifiers.push(qualifier);

        self
    }
}

pub struct Method {
    attrs: Vec<Attr>,
    vis: Option<Vis>,
    qualifiers: Vec<Qualifier>,
    ret: Type,
    args: Vec<(Type, String)>,
    name: String,
    body: Option<Block>,
}

pub enum Block {
    Empty,
    Unsafe,
    Fixed,
}

pub struct Attr {
    name: String,
    args: Vec<AttrArg>,
}

pub enum AttrArg {
    Value(String),
    ArgValue((String, String)),
}

pub struct Variable {
    qualifiers: Vec<Qualifier>,
    ty: Type,
    vis: Vis,
    val: String,
    name: String,
}

pub struct VariableBuilder {
    qualifiers: Vec<Qualifier>,
    ty: Option<Type>,
    val: Option<String>,
    vis: Option<Vis>,
    name: String,
}

impl VariableBuilder {
    pub fn new(name: String) -> Self {
        Self {
            qualifiers: vec![],
            ty: None,
            val: None,
            vis: None,
            name,
        }
    }

    pub fn ty(mut self, ty: Type) -> Self {
        self.ty = Some(ty);
        self
    }

    pub fn val(mut self, val: String) -> Self {
        self.val = Some(val);
        self
    }

    pub fn qualifier(mut self, qualifier: Qualifier) -> Self {
        self.qualifiers.push(qualifier);
        self
    }

    pub fn vis(mut self, vis: Vis) -> Self {
        self.vis = Some(vis);
        self
    }

    pub fn build(self) -> Variable {
        Variable {
            name: self.name,
            qualifiers: self.qualifiers,
            ty: self.ty.unwrap(),
            vis: self.vis.unwrap(),
            val: self.val.unwrap(),
        }
    }
}

impl ToString for Class {
    fn to_string(&self) -> String {
        let vis = if let Some(vis) = &self.vis {
            vis.as_str()
        } else {
            ""
        };

        let qualifiers: Vec<&str> = self.qualifiers.iter().map(|s| s.as_str()).collect();

        let mut methods = Vec::new();

        let layer = 1;

        let indents = "\t".repeat(layer);

        for method in &self.methods {
            let vis = if let Some(vis) = &method.vis {
                vis.as_str()
            } else {
                ""
            };

            let mut args = Vec::new();

            for (ty, name) in &method.args {
                args.push(format!("{} {name}", ty.to_string()));
            }

            // methods.append(Group::new(proc_macro2::Delimiter::Parenthesis, args));

            let body = if let Some(body) = &method.body {
                todo!()
            } else {
                ";"
            };

            let strings = method
                .qualifiers
                .iter()
                .fold(String::new(), |acc, s| format!("{acc} {}", s.as_str()));

            let args = format!("{}", args.join(", "));

            let mut attrs = Vec::new();

            for attr in &method.attrs {
                let args: Vec<String> = attr
                    .args
                    .iter()
                    .map(|arg| match arg {
                        AttrArg::Value(v) => format!("{v}"),
                        AttrArg::ArgValue((name, value)) => format!("{name} = {value}"),
                    })
                    .collect();

                let arg_str = args.join(", ");

                let attr = format!("[{}({arg_str})]", attr.name);
                attrs.push(attr);
            }

            // YOU NEED TO HANDLE METHODS AND BLOCKS RECURSIVELY IN A WAY THAT LETS YOU TRACK INDENTATION PLEASE DO NOT FORGET WHAT YOU MEAN
            let method = format!(
                "{indents}{attrs}\n{indents}{vis}{qualifiers} {ret} {name}({args}){body}\n",
                attrs = attrs.join("\n"),
                qualifiers = strings,
                name = method.name,
                ret = method.ret.to_string(),
            );

            methods.push(method);
        }

        let constants = self.constants.iter().fold(String::new(), |acc, constant| {
            format!(
                "{acc}{indents}{}{} const {} {} = {};",
                constant.vis.as_str(),
                constant
                    .qualifiers
                    .iter()
                    .fold(String::new(), |acc, s| format!("{acc} {}", s.as_str())),
                constant.ty.to_string(),
                constant.name,
                constant.val,
            )
        });

        let class = format!(
            "{vis} {} class {} {{\n{}\n{}}}",
            qualifiers.join(" "),
            self.name,
            constants,
            methods.join("\n")
        );

        class
    }
}

#[test]
fn test() {
    let class = Class {
        constants: vec![],
        vis: Some(Vis::Public),
        qualifiers: vec![Qualifier::Static],
        name: "NativeMethods".into(),
        methods: vec![
            Method {
                attrs: vec![Attr {
                    name: "DllImport".into(),
                    args: vec![
                        AttrArg::Value("__DLLName".into()),
                        AttrArg::ArgValue(("EntryPoint".into(), "\"malloc\"".into())),
                    ],
                }],
                vis: Some(Vis::Public),
                qualifiers: vec![Qualifier::Static, Qualifier::Extern],
                ret: Type::Ptr(Box::new(Type::Void)),
                args: vec![(Type::Nuint, "size".into())],
                name: "malloc".into(),
                body: None,
            },
            Method {
                attrs: vec![Attr {
                    name: "DllImport".into(),
                    args: vec![
                        AttrArg::Value("__DLLName".into()),
                        AttrArg::ArgValue(("EntryPoint".into(), "\"free\"".into())),
                    ],
                }],
                vis: Some(Vis::Public),
                qualifiers: vec![Qualifier::Static, Qualifier::Unsafe, Qualifier::Extern],
                ret: Type::Void,
                args: vec![
                    (Type::Ptr(Box::new(Type::Void)), "ptr".into()),
                    (Type::Nuint, "size".into()),
                ],
                name: "free".into(),
                body: None,
            },
        ],
    };

    println!("{}", class.to_string())
}
