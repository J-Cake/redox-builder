use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo::rustc-link-search=fat32/build");
    println!("cargo::rustc-link-lib=fat32");
    println!("cargo::rerun-if-changed=fat32-fuse/vfat.c");

    // TODO: Build FUSE driver here too

    let gen = bindgen::Builder::default()
        .header("fat32-fuse/vfat.c")
        .clang_arg("-std=c99")
        .clang_arg("-D_FILE_OFFSET_BITS=64")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Failed to build Fat32-Fuse");

    let emit = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    println!("Out Dir: {:?}", emit);

    gen.write_to_file(emit.join("bindings.rs"))
        .expect("Failed to write bindings");
}