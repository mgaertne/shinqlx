fn main() {
    let python_config = pyo3_build_config::get();

    if let Some(lib_dir) = &python_config.lib_dir {
        println!("cargo:rustc-link-search={}", lib_dir);
    }

    if let Some(library) = &python_config.lib_name {
        println!("cargo:rustc-link-lib={}", library);
    }

    #[cfg(target_os = "linux")]
    {
        let mut builder = cc::Build::new();
        builder.files([
            "src/hooks.c",
            "src/simple_hook.c",
            "src/trampoline.c",
            #[cfg(target_pointer_width = "64")]
            "src/HDE/hde64.c",
            #[cfg(target_pointer_width = "32")]
            "src/HDE/hde32.c",
        ]);
        let shinqlx_version = format!(
            "\"v{}-{}\"",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_NAME")
        );
        builder.define("MINQLX_VERSION", shinqlx_version.as_str());
        builder
            .flag("-shared")
            .flag("-std=gnu11")
            .flag("-Wno-unused-variable")
            .flag("-Wno-unused-parameter")
            .flag("-Wno-stringop-truncation");

        #[cfg(debug_assertions)]
        builder
            .flag("-gdwarf-2")
            .flag("-Wall")
            .flag("-O0")
            .flag("-fvar-tracking");

        builder.compile("minqlx");

        println!("cargo:rerun-if-changed=src/quake_types.h");
        println!("cargo:rerun-if-changed=src/hooks.c");
        println!("cargo:rerun-if-changed=src/simple_hook.c");
        println!("cargo:rerun-if-changed=src/simple_hook.h");
        println!("cargo:rerun-if-changed=src/trampoline.c");
        println!("cargo:rerun-if-changed=src/trampoline.h");
        #[cfg(target_pointer_width = "64")]
        println!("cargo:rerun-if-changed=src/HDE/hde64.c");
        println!("cargo:rerun-if-changed=src/HDE/hde64.h");
        println!("cargo:rerun-if-changed=src/HDE/table64.h");
        #[cfg(target_pointer_width = "32")]
        println!("cargo:rerun-if-changed=src/HDE/hde32.c");
        println!("cargo:rerun-if-changed=src/HDE/hde32.h");
        println!("cargo:rerun-if-changed=src/HDE/table32.h");
    }
}
