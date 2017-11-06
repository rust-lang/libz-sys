extern crate pkg_config;
#[cfg(target_env = "msvc")]
extern crate vcpkg;
extern crate cc;

use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::prelude::*;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(n) => n,
        Err(e) => panic!("\n{} failed with {}\n", stringify!($e), e),
    })
}

fn main() {
    let host = env::var("HOST").unwrap();
    let target = env::var("TARGET").unwrap();

    let host_and_target_contain = |s| host.contains(s) && target.contains(s);

    // Don't run pkg-config if we're linking statically (we'll build below) and
    // also don't run pkg-config on macOS/FreeBSD/DragonFly. That'll end up printing
    // `-L /usr/lib` which wreaks havoc with linking to an OpenSSL in /usr/local/lib
    // (Homebrew, Ports, etc.)
    let want_static = env::var("LIBZ_SYS_STATIC").unwrap_or(String::new()) == "1";
    if !want_static &&
       !(host_and_target_contain("apple") ||
         host_and_target_contain("freebsd") ||
         host_and_target_contain("dragonfly")) &&
        pkg_config::find_library("zlib").is_ok() {
        return
    }

    // Practically all platforms come with libz installed already, but MSVC is
    // one of those sole platforms that doesn't!
    if target.contains("msvc") {
        if try_vcpkg() {
            return;
        }

        build_msvc_zlib(&target);
    } else if target.contains("pc-windows-gnu") {
        build_zlib_mingw();
    } else if (target.contains("musl") ||
               target != host ||
               want_static) &&
              !target.contains("windows-gnu") &&
              !target.contains("android") {
        build_zlib();
    } else {
        println!("cargo:rustc-link-lib=z");
    }
}

fn build_zlib() {
    let src = env::current_dir().unwrap().join("src/zlib");
    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let build = dst.join("build");
    t!(fs::create_dir_all(&build));
    cp_r(&src, &build);
    let compiler = cc::Build::new().get_compiler();
    let mut cflags = OsString::new();
    for arg in compiler.args() {
        cflags.push(arg);
        cflags.push(" ");
    }
    run(Command::new("./configure")
                .current_dir(&build)
                .env("CC", compiler.path())
                .env("CFLAGS", cflags)
                .arg(format!("--prefix={}", dst.display())), "sh");
    run(make()
            .current_dir(&build)
            .arg("libz.a"), "make");

    t!(fs::create_dir_all(dst.join("lib/pkgconfig")));
    t!(fs::create_dir_all(dst.join("include")));
    t!(fs::copy(build.join("libz.a"), dst.join("lib/libz.a")));
    t!(fs::copy(build.join("zlib.h"), dst.join("include/zlib.h")));
    t!(fs::copy(build.join("zconf.h"), dst.join("include/zconf.h")));
    t!(fs::copy(build.join("zlib.pc"), dst.join("lib/pkgconfig/zlib.pc")));

    println!("cargo:rustc-link-lib=static=z");
    println!("cargo:rustc-link-search={}/lib", dst.to_string_lossy());
    println!("cargo:root={}", dst.to_string_lossy());
    println!("cargo:include={}/include", dst.to_string_lossy());
}

fn make() -> Command {
    let cmd = if cfg!(any(target_os = "freebsd", target_os = "dragonfly")) {"gmake"} else {"make"};
    let mut cmd = Command::new(cmd);

    // We're using the MSYS make which doesn't work with the mingw32-make-style
    // MAKEFLAGS, so remove that from the env if present.
    if cfg!(windows) {
        cmd.env_remove("MAKEFLAGS").env_remove("MFLAGS");
    } else if let Some(makeflags) = env::var_os("CARGO_MAKEFLAGS") {
        cmd.env("MAKEFLAGS", makeflags);
    }

    return cmd
}

// We have to run a few shell scripts, which choke quite a bit on both `\`
// characters and on `C:\` paths, so normalize both of them away.
fn sanitize_sh(path: &Path) -> String {
    let path = path.to_str().unwrap().replace("\\", "/");
    return change_drive(&path).unwrap_or(path);

    fn change_drive(s: &str) -> Option<String> {
        let mut ch = s.chars();
        let drive = ch.next().unwrap_or('C');
        if ch.next() != Some(':') {
            return None
        }
        if ch.next() != Some('/') {
            return None
        }
        Some(format!("/{}/{}", drive, &s[drive.len_utf8() + 2..]))
    }
}

fn build_zlib_mingw() {
    let src = env::current_dir().unwrap().join("src/zlib");
    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let build = dst.join("build");
    t!(fs::create_dir_all(&build));
    cp_r(&src, &build);
    let compiler = cc::Build::new().get_compiler();
    let mut cflags = OsString::new();
    for arg in compiler.args() {
        cflags.push(arg);
        cflags.push(" ");
    }
    cflags.push("-Wno-error ");
    let gcc = sanitize_sh(compiler.path());
    let mut cmd = make();
    cmd.arg("-f").arg("win32/Makefile.gcc")
       .current_dir(&build)
       .arg("install")
       .arg(format!("prefix={}", sanitize_sh(&dst)))
       .arg("IMPLIB=")
       .arg(format!("INCLUDE_PATH={}", sanitize_sh(&dst.join("include"))))
       .arg(format!("LIBRARY_PATH={}", sanitize_sh(&dst.join("lib"))))
       .arg(format!("BINARY_PATH={}", sanitize_sh(&dst.join("bin"))))
       .env("CFLAGS", cflags);

    if gcc != "gcc" {
        match gcc.find("gcc") {
            Some(0) => {}
            Some(i) => {
                cmd.arg(format!("PREFIX={}", &gcc[..i]));
            }
            None => {}
        }
    }
    run(&mut cmd, "make");

    t!(fs::create_dir_all(dst.join("lib/pkgconfig")));

    println!("cargo:rustc-link-lib=static=z");
    println!("cargo:rustc-link-search={}/lib", dst.to_string_lossy());
    println!("cargo:root={}", dst.to_string_lossy());
    println!("cargo:include={}/include", dst.to_string_lossy());
}

fn cp_r(dir: &Path, dst: &Path) {
    for entry in t!(fs::read_dir(dir)) {
        let entry = t!(entry);
        let path = entry.path();
        let dst = dst.join(path.file_name().unwrap());
        if t!(fs::metadata(&path)).is_file() {
            t!(fs::copy(path, dst));
        } else {
            t!(fs::create_dir_all(&dst));
            cp_r(&path, &dst);
        }
    }
}

fn build_msvc_zlib(target: &str) {
    let src = t!(env::current_dir()).join("src/zlib");
    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    t!(fs::create_dir_all(dst.join("lib")));
    t!(fs::create_dir_all(dst.join("include")));
    t!(fs::create_dir_all(dst.join("build")));
    cp_r(&src, &dst.join("build"));

    let features = env::var("CARGO_CFG_TARGET_FEATURE")
                      .unwrap_or(String::new());
    if features.contains("crt-static") {
        let mut makefile = String::new();
        let makefile_path = dst.join("build/win32/Makefile.msc");
        t!(t!(File::open(&makefile_path)).read_to_string(&mut makefile));
        let new_makefile = makefile.replace(" -MD ", " -MT ");
        t!(t!(File::create(&makefile_path)).write_all(new_makefile.as_bytes()));
    }

    let nmake = cc::windows_registry::find(target, "nmake.exe");
    let mut nmake = nmake.unwrap_or(Command::new("nmake.exe"));

    // These env vars are intended for mingw32-make, not `namek`, which chokes
    // on them anyway.
    nmake.env_remove("MAKEFLAGS")
         .env_remove("MFLAGS");

    run(nmake.current_dir(dst.join("build"))
             .arg("/nologo")
             .arg("/f")
             .arg(dst.join("build/win32/Makefile.msc"))
             .arg("zlib.lib"), "nmake.exe");

    for file in t!(fs::read_dir(&dst.join("build"))) {
        let file = t!(file).path();
        if let Some(s) = file.file_name().and_then(|s| s.to_str()) {
            if s.ends_with(".h") {
                t!(fs::copy(&file, dst.join("include").join(s)));
            }
        }
    }
    t!(fs::copy(dst.join("build/zlib.lib"), dst.join("lib/zlib.lib")));

    println!("cargo:rustc-link-lib=static=zlib");
    println!("cargo:rustc-link-search={}/lib", dst.to_string_lossy());
    println!("cargo:root={}", dst.to_string_lossy());
    println!("cargo:include={}/include", dst.to_string_lossy());
}

fn run(cmd: &mut Command, program: &str) {
    println!("running: {:?}", cmd);
    let status = match cmd.status() {
        Ok(status) => status,
        Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
            fail(&format!("failed to execute command: {}\nIs `{}` \
                           not installed?",
                          e,
                          program));
        }
        Err(e) => fail(&format!("failed to execute command: {}", e)),
    };
    if !status.success() {
        fail(&format!("command did not execute successfully, got: {}", status));
    }
}

fn fail(s: &str) -> ! {
    println!("\n\n{}\n\n", s);
    std::process::exit(1);
}

#[cfg(not(target_env = "msvc"))]
fn try_vcpkg() -> bool {
    false
}

#[cfg(target_env = "msvc")]
fn try_vcpkg() -> bool {
    // see if there is a vcpkg tree with zlib installed
    match vcpkg::Config::new()
            .emit_includes(true)
            .lib_names("zlib", "zlib1")
            .probe("zlib") {
        Ok(_) => { true },
        Err(e) => {
            println!("note, vcpkg did not find zlib: {}", e);
            false
        },
    }
}
