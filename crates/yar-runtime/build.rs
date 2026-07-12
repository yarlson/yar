fn main() {
    println!("cargo:rerun-if-changed=src/gc_roots.c");
    println!("cargo:rerun-if-changed=src/runtime_exit.c");
    let mut build = cc::Build::new();
    build
        .file("src/gc_roots.c")
        .file("src/runtime_exit.c")
        .warnings(true)
        .flag_if_supported("-fno-sanitize=all");
    if std::env::var("CARGO_CFG_TARGET_FAMILY").as_deref() != Ok("windows") {
        build.flag_if_supported("-fPIC");
    }
    build.compile("yar_gc_roots");
}
