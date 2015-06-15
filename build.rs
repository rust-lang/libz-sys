extern crate pkg_config;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

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
    let src = env::current_dir().unwrap().join("src/zlib-1.2.8");
    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    fs::create_dir(dst.join("lib")).unwrap();
    fs::create_dir(dst.join("include")).unwrap();

    let mut top = OsString::from("TOP=");
    top.push(&src);
    run(Command::new("nmake")
                .current_dir(dst.join("lib"))
                .arg("/f")
                .arg(src.join("win32/Makefile.msc"))
                .arg(top)
                .arg("zlib.lib"));

    for file in fs::read_dir(&src).unwrap() {
        let file = file.unwrap().path();
        if let Some(s) = file.file_name().and_then(|s| s.to_str()) {
            if s.ends_with(".h") {
                fs::copy(&file, dst.join("include").join(s)).unwrap();
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
