extern crate "pkg-config" as pkg_config;

fn main() {
    match pkg_config::find_library("zlib") {
        Ok(..) => return,
        Err(..) => {}
    }

    println!("cargo:rustc-flags=-l z");
}
