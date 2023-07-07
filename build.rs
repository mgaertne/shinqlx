fn main() {
    let python_config = pyo3_build_config::get();

    if let Some(lib_dir) = &python_config.lib_dir {
        println!("cargo:rustc-link-search={}", lib_dir);
    }

    if let Some(library) = &python_config.lib_name {
        println!("cargo:rustc-link-lib={}", library);
    }
}
