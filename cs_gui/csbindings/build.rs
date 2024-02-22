fn main() {
    csbindgen::Builder::new()
        .input_extern_file("src/lib.rs")
        .input_extern_file("src/tasks.rs")
        .csharp_dll_name("csbindings")
        .csharp_class_accessibility("public")
        .generate_csharp_file("../NativeMethods.cs")
        .unwrap();
}
