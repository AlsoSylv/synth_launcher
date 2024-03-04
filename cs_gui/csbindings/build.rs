fn main() {
    csbindgen::Builder::new()
        .input_extern_file("src/lib.rs")
        .input_extern_file("src/internal/tasks.rs") // Need the task wrapper and State structs
        .input_extern_file("src/internal/state.rs")
        .csharp_dll_name("csbindings")
        .csharp_class_accessibility("public")
        .generate_csharp_file("../NativeMethods.cs")
        .unwrap();
}
