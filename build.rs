extern crate pkg_config;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(n) => n,
        Err(e) => panic!("\n{} failed with {}\n", stringify!($e), e),
    })
}

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
    let src = t!(env::current_dir()).join("src/zlib-1.2.8");
    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    t!(fs::create_dir_all(dst.join("lib")));
    t!(fs::create_dir_all(dst.join("include")));

    let mut top = OsString::from("TOP=");
    top.push(&src);
    run(Command::new("nmake")
                .current_dir(dst.join("lib"))
                .arg("/f")
                .arg(src.join("win32/Makefile.msc"))
                .arg(top)
                .arg("zlib.lib"));

    for file in t!(fs::read_dir(&src)) {
        let file = t!(file).path();
        if let Some(s) = file.file_name().and_then(|s| s.to_str()) {
            if s.ends_with(".h") {
                t!(fs::copy(&file, dst.join("include").join(s)));
            }
        }
    }

    println!("cargo:rustc-link-lib=zlib");
    println!("cargo:rustc-link-search={}/lib", dst.to_string_lossy());
    println!("cargo:root={}", dst.to_string_lossy());
    println!("cargo:include={}/include", dst.to_string_lossy());
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
