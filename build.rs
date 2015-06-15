extern crate pkg_config;

use std::ffi::OsString;
use std::process::Command;
use std::env;

fn main() {
    if pkg_config::find_library("zlib").is_ok() {
        return
    }

    // Practically all platforms come with libz installed already, but MSVC is
    // one of those sole platforms that doesn't!
    let target = env::var("TARGET").unwrap();
    if target.contains("msvc") {
        build_msvc_zlib();
    } else {
        println!("cargo:rustc-link-lib=z");
    }
}

fn build_msvc_zlib() {
    let src = env::current_dir().unwrap();
    let dst = env::var_os("OUT_DIR").unwrap();

    let mut top = OsString::from("TOP=");
    top.push(&src);
    top.push("/src/zlib-1.2.8");
    run(Command::new("nmake")
                .current_dir(&dst)
                .arg("/f")
                .arg(src.join("src/zlib-1.2.8/win32/Makefile.msc"))
                .arg(top)
                .arg("zlib.lib"));
    println!("cargo:rustc-link-lib=zlib");
    println!("cargo:rustc-link-search={}", dst.to_string_lossy());
}

fn run(cmd: &mut Command) {
    println!("running: {:?}", cmd);
    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => panic!("failed to run: {}", e),
    };
    if !status.success() {
        panic!("failed to run successfully: {}", status);
    }
}
