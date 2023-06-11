use std::env;

fn main() {
    let zng = env::var("CARGO_PKG_NAME").unwrap() == "systest-zng";
    let mut cfg = ctest2::TestGenerator::new();
    cfg.define("WITH_GZFILEOP", Some("ON"));
    let (header, dep_include) = if zng {
        ("zlib-ng.h", "DEP_Z_NG_INCLUDE")
    } else {
        ("zlib.h", "DEP_Z_INCLUDE")
    };
    cfg.header(header);
    if let Some(s) = env::var_os(dep_include) {
        cfg.include(s);
    }
    if zng {
        println!("cargo:rustc-cfg=zng");

        // The link_name argument does not seem to get populated.
        cfg.fn_cname(|rust, _| {
            if rust == "zlibVersion" {
                return "zlibng_version".to_string();
            }
            if rust.starts_with("zng_") {
                rust.to_string()
            } else {
                format!("zng_{}", rust)
            }
        });
        cfg.cfg("zng", None);
    }
    cfg.type_name(move |n, _, _| {
        if zng {
            if n == "gz_header" || n == "gz_headerp" {
                return format!("zng_{}", n);
            } else if n == "z_stream" {
                return "zng_stream".to_string();
            } else if n == "z_streamp" {
                return "zng_streamp".to_string();
            } else if n == "z_size" {
                return "size_t".to_string();
            } else if n == "z_checksum" {
                return "uint32_t".to_string();
            } else if n == "z_off_t" {
                return "z_off64_t".to_string();
            }
        } else {
            if n == "z_size" {
                return "unsigned long".to_string();
            } else if n == "z_checksum" {
                return "unsigned long".to_string();
            }
        }
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
