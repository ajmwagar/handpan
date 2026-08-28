fn main() {
    cxx_build::bridge("src/lib.rs")
        .flag_if_supported("-std=c++17")
        .compile("vcv-handpan-cxx");
    println!("cargo:rerun-if-changed=src/lib.rs");
}
