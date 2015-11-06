extern crate ctest;

use std::env;

fn main() {
    let mut cfg = ctest::TestGenerator::new();
    cfg.header("zlib.h");
    if let Some(s) = env::var_os("DEP_Z_INCLUDE") {
        cfg.include(s);
    }
    cfg.type_name(|n, _| {
        if n == "internal_state" {
            format!("struct {}", n)
        } else {
            n.to_string()
        }
    });
    cfg.skip_signededness(|ty| {
        match ty  {
            "gz_headerp" |
            "voidpf" |
            "voidcf" |
            "voidp" |
            "out_func" |
            "voidpc" |
            "gzFile" |
            "in_func" |
            "free_func" |
            "alloc_func" |
            "z_streamp" => true,
            _ => false,
        }
    });
    cfg.generate("../src/lib.rs", "all.rs");
}

