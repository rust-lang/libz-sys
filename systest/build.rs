extern crate ctest2;

use std::env;

fn main() {
    let mut cfg = ctest2::TestGenerator::new();
    cfg.header("zlib.h");
    if let Some(s) = env::var_os("DEP_Z_INCLUDE") {
        cfg.include(s);
    }
    cfg.type_name(|n, _, _| {
        if n == "internal_state" {
            format!("struct {}", n)
        } else {
            n.to_string()
        }
    });
    cfg.skip_signededness(|ty| match ty {
        "gz_headerp" | "voidpf" | "voidcf" | "voidp" | "out_func" | "voidpc" | "gzFile"
        | "in_func" | "free_func" | "alloc_func" | "z_streamp" => true,
        _ => false,
    });
    cfg.skip_field_type(|s, field| s == "z_stream" && (field == "next_in" || field == "msg"));
    cfg.generate("../src/lib.rs", "all.rs");
}
