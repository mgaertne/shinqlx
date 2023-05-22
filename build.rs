#![allow(unused_imports)]

use std::process::Command;

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
fn main() {
    let mut includes = vec![];
    match Command::new("python3-config").args(["--includes"]).output() {
        Err(_) => {
            println!("You need to install python3-config installed to compile");
            return;
        }
        Ok(python_includes_output) => {
            let includes_output = String::from_utf8(python_includes_output.stdout).unwrap();
            for include in includes_output.split(' ') {
                includes.push(include.replace("-I", ""));
            }
        }
    }
    let mut builder = cc::Build::new();
    builder.files([
        "src/dllmain.c",
        "src/hooks.c",
        "src/commands.c",
        "src/python_embed.c",
        "src/python_dispatchers.c",
        "src/simple_hook.c",
        "src/misc.c",
        "src/maps_parser.c",
        "src/trampoline.c",
        "src/patches.c",
        #[cfg(target_pointer_width = "64")]
        "src/HDE/hde64.c",
        #[cfg(target_pointer_width = "32")]
        "src/HDE/hde32.c",
    ]);
    builder.includes(includes);
    let shinqlx_version = format!(
        "\"v{}-{}\"",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_NAME")
    );
    builder
        .flag("-shared")
        .flag("-std=gnu11")
        .flag("-Wno-unused-variable")
        .flag("-Wno-unused-parameter")
        .flag("-Wno-stringop-truncation")
        .define("MINQLX_VERSION", shinqlx_version.as_str());

    #[cfg(debug_assertions)]
    builder
        .flag("-gdwarf-2")
        .flag("-Wall")
        .flag("-O0")
        .flag("-fvar-tracking");

    builder.compile("minqlx");

    println!("cargo:rerun-if-changed=src/dllmain.c");
    println!("cargo:rerun-if-changed=src/commands.c");
    println!("cargo:rerun-if-changed=src/python_embed.c");
    println!("cargo:rerun-if-changed=src/python_dispatchers.c");
    println!("cargo:rerun-if-changed=src/simple_hook.c");
    println!("cargo:rerun-if-changed=src/hooks.c");
    println!("cargo:rerun-if-changed=src/misc.c");
    println!("cargo:rerun-if-changed=src/maps_parser.c");
    println!("cargo:rerun-if-changed=src/trampoline.c");
    println!("cargo:rerun-if-changed=src/patches.c");
    #[cfg(target_pointer_width = "64")]
    println!("cargo:rerun-if-changed=src/HDE/hde64.c");
    #[cfg(target_pointer_width = "32")]
    println!("cargo:rerun-if-changed=src/HDE/hde32.c");

    if let Ok(libs) = Command::new("python3-config")
        .args(["--libs", "--embed"])
        .output()
    {
        let libs_embed_output = String::from_utf8(libs.stdout).unwrap();
        for lib in libs_embed_output.split(' ') {
            if lib.replace("-l", "").trim() != "" {
                println!("cargo:rustc-link-lib={}", lib.replace("-l", ""));
            }
        }
    } else if let Ok(libs) = Command::new("python3-config").args(["--libs"]).output() {
        let libs_embed_output = String::from_utf8(libs.stdout).unwrap();
        for lib in libs_embed_output.split(' ') {
            if lib.replace("-l", "").trim() != "" {
                println!("cargo:rustc-link-lib={}", lib.replace("-l", ""));
            }
        }
    }
}
