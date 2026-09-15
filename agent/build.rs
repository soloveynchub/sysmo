fn main() {
    cc::Build::new().file("src/native.c").file("src/thermal.m").compile("native");
    println!("cargo:rustc-link-lib=framework=IOKit");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    println!("cargo:rerun-if-changed=src/native.c");
    println!("cargo:rerun-if-changed=src/thermal.m");
}
