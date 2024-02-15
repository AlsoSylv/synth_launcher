fn main() {
    csbindgen::Builder::new()
        .input_extern_file("src/lib.rs")
        .csharp_dll_name("csbindings")
        .generate_csharp_file("../NativeMethods.cs")
        .unwrap();
}
