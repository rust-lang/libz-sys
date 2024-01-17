use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    hash::{Hash as _, Hasher as _},
    io::Write as _,
    path::{Path, PathBuf},
};

fn append(cfg: &mut cc::Build, root: Option<&str>, files: impl IntoIterator<Item = &'static str>) {
    let root = root.unwrap_or("");
    cfg.files(
        files
            .into_iter()
            .map(|fname| format!("src/zlib-ng/{root}{fname}.c")),
    );
}

/// Replicate the behavior of cmake/make/configure of stripping out the
/// @ZLIB_SYMBOL_PREFIX@ since we don't want or need it
fn strip_symbol_prefix(input: &Path, output: &Path, mut on_line: impl FnMut(&str)) {
    let contents = fs::read_to_string(input).unwrap();
    let mut h = fs::File::create(output).expect("failed to create zlib include");

    use std::io::IoSlice;
    let mut write = |bufs: &[IoSlice]| {
        // write_all_vectored is unstable
        for buf in bufs {
            h.write_all(&buf).unwrap();
        }
    };

    for line in contents.lines() {
        if let Some((begin, end)) = line.split_once("@ZLIB_SYMBOL_PREFIX@") {
            write(&[
                IoSlice::new(begin.as_bytes()),
                IoSlice::new(end.as_bytes()),
                IoSlice::new(b"\n"),
            ]);
        } else {
            write(&[IoSlice::new(line.as_bytes()), IoSlice::new(b"\n")]);
        }

        on_line(line);
    }
}

#[derive(Default)]
struct AppendState {
    flags: BTreeSet<&'static str>,
    defines: BTreeSet<&'static str>,
    files: BTreeSet<&'static str>,
}

#[derive(Debug, Hash, Copy, Clone)]
struct TargetFeature {
    check: &'static str,
    msvc_flags: &'static [&'static str],
    flags: &'static [&'static str],
    defines: &'static [&'static str],
    files: &'static [&'static str],
}

struct Ctx<'ctx> {
    cfg: &'ctx mut cc::Build,
    root: Option<&'ctx str>,
    enabled: Option<&'ctx BTreeSet<&'ctx str>>,
    msvc: bool,
    append: AppendState,
    cache: Option<BTreeMap<u64, bool>>,
}

impl<'ctx> Ctx<'ctx> {
    fn enable(&mut self, tf: TargetFeature) -> bool {
        if let Some(enabled) = self.enabled {
            if !enabled.contains(tf.check) {
                return false;
            }
        }

        self.push(tf);
        true
    }

    fn compile_check(&mut self, subdir: &str, tf: TargetFeature) -> bool {
        let dst = Self::compile_check_root();
        fs::create_dir_all(&dst).unwrap();

        if let Some(res) = self.check_cache(&tf) {
            println!("cargo:warning=using cached compile check");
            if res {
                self.push(tf);
            }

            return res;
        }

        let mut cmd = self.cfg.get_compiler().to_command();

        // compile only
        cmd.arg(if self.msvc { "/c" } else { "-c" });

        // set output file so we don't pollute the cwd
        {
            let path = {
                let mut pb = dst.join(tf.check);
                pb.set_extension(if self.msvc { "obj" } else { "o" });
                pb
            };
            if self.msvc {
                cmd.arg(format!("/Fo\"{}\"", path.display()));
            } else {
                cmd.arg("-o");
                cmd.arg(path);
            }
        }

        let flags = if self.msvc { tf.msvc_flags } else { tf.flags };
        cmd.args(flags);

        let path = {
            let mut p = "src/compile_check/".to_owned();
            if !subdir.is_empty() {
                p.push_str(subdir);
                p.push('/');
            }

            p.push_str(tf.check);
            p.push_str(".c");
            p
        };

        cmd.arg(path);

        let output = cmd.output().expect("failed to run compiler");

        let result = if !output.status.success() {
            println!("cargo:warning=failed to compile check '{tf:?}'");

            for line in String::from_utf8(output.stderr).unwrap().lines() {
                println!("cargo:warning={line}");
            }

            false
        } else {
            self.push(tf);
            true
        };

        self.insert_cache(tf, result);
        result
    }

    fn push(&mut self, tf: TargetFeature) {
        println!("cargo:warning=enabling {tf:?}");

        let flags = if self.msvc { tf.msvc_flags } else { tf.flags };

        self.append.flags.extend(flags);
        self.append.defines.extend(tf.defines);
        self.append.files.extend(tf.files);
    }

    fn check_cache(&mut self, tf: &TargetFeature) -> Option<bool> {
        let cache = self.cache.get_or_insert_with(|| {
            let mut cache = Self::compile_check_root();
            cache.push("cache.txt");

            let Ok(cache_contents) = fs::read_to_string(cache) else {
                return BTreeMap::new();
            };

            let mut cache = BTreeMap::new();
            for line in cache_contents.lines() {
                let Some((hash, res)) = line.split_once(' ') else {
                    continue;
                };
                let Ok(hash) = u64::from_str_radix(hash, 16) else {
                    continue;
                };
                let res = match res {
                    "1" => true,
                    "0" => false,
                    _ => continue,
                };

                cache.insert(hash, res);
            }

            cache
        });

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        tf.hash(&mut hasher);

        cache.get(&hasher.finish()).cloned()
    }

    fn insert_cache(&mut self, tf: TargetFeature, res: bool) {
        let cache = self.cache.as_mut().unwrap();

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        tf.hash(&mut hasher);
        cache.insert(hasher.finish(), res);
    }

    fn compile_check_root() -> PathBuf {
        let mut pb = PathBuf::from(env::var_os("OUT_DIR").unwrap());
        pb.push("compile_check");
        pb
    }
}

impl<'ctx> Drop for Ctx<'ctx> {
    fn drop(&mut self) {
        let app = std::mem::replace(&mut self.append, Default::default());
        for flag in app.flags {
            self.cfg.flag(flag);
        }

        for def in app.defines {
            self.cfg.define(def, None);
        }

        append(self.cfg, self.root, app.files);

        if let Some(cache) = self.cache.take() {
            let mut cstr = String::new();
            for (hash, res) in cache {
                use std::fmt::Write as _;
                writeln!(&mut cstr, "{hash:08x} {}", if res { "1" } else { "0" }).unwrap();
            }

            fs::write(
                Self::compile_check_root().join("cache.txt"),
                cstr.as_bytes(),
            )
            .expect("failed to write cache.txt");
        }
    }
}

pub fn build_zlib_ng(target: &str, compat: bool) {
    let mut cfg = cc::Build::new();

    let dst = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let lib = dst.join("lib");
    cfg.warnings(false).out_dir(&lib);

    append(
        &mut cfg,
        None,
        [
            "adler32",
            "adler32_fold",
            "chunkset",
            "compare256",
            "compress",
            "cpu_features",
            "crc32_braid",
            "crc32_braid_comb",
            "crc32_fold",
            "deflate",
            "deflate_fast",
            "deflate_huff",
            "deflate_medium",
            "deflate_quick",
            "deflate_rle",
            "deflate_slow",
            "deflate_stored",
            "functable",
            // GZFILEOP
            "gzlib",
            "gzwrite",
            "infback",
            "inflate",
            "inftrees",
            "insert_string",
            "insert_string_roll",
            "slide_hash",
            "trees",
            "uncompr",
            "zutil",
        ],
    );

    if compat {
        cfg.define("ZLIB_COMPAT", None);
    }

    cfg.define("WITH_GZFILEOP", None);

    {
        let mut build = dst.join("build");
        fs::create_dir_all(&build).unwrap();
        build.push("gzread.c");

        strip_symbol_prefix(Path::new("src/zlib-ng/gzread.c.in"), &build, |_line| {});
        cfg.file(build);
    }

    let msvc = target.ends_with("pc-windows-msvc");

    cfg.std("c11");

    // This can be made configurable if it is an issue but most of these would
    // only fail if the user was on a decade old+ libc impl
    if !msvc {
        cfg.define("HAVE_ALIGNED_ALLOC", None)
            .define("HAVE_ATTRIBUTE_ALIGNED", None)
            .define("HAVE_BUILTIN_CTZ", None)
            .define("HAVE_BUILTIN_CTZLL", None)
            .define("HAVE_POSIX_MEMALIGN", None)
            .define("HAVE_THREAD_LOCAL", None)
            .define("HAVE_VISIBILITY_HIDDEN", None)
            .define("HAVE_VISIBILITY_INTERNAL", None);
    }

    if !target.contains("windows") {
        cfg.define("STDC", None)
            .define("_LARGEFILE64_SOURCE", "1")
            .define("__USE_LARGEFILE64", None)
            .define("_POSIX_SOURCE", None)
            .flag("-fvisibility=hidden");
    }
    if target.contains("apple") {
        cfg.define("_C99_SOURCE", None);
    }
    if target.contains("solaris") {
        cfg.define("_XOPEN_SOURCE", "700");
    }

    if target.contains("s390x") {
        // Enable hardware compression on s390x.
        cfg.file("src/zlib-ng/arch/s390/dfltcc_deflate.c")
            .flag("-DDFLTCC_LEVEL_MASK=0x7e");
    }

    let tf = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    let target_features: BTreeSet<_> = tf.split(',').collect();

    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("failed to retrieve target arch");
    match arch.as_str() {
        "x86_64" | "i686" => {
            cfg.define("X86_FEATURES", None);
            cfg.file("src/zlib-ng/arch/x86/x86_features.c");

            let is_64 = arch.as_str() == "x86_64";

            let pclmulqdq = {
                let mut ctx = Ctx {
                    cfg: &mut cfg,
                    root: Some("arch/x86/"),
                    enabled: Some(&target_features),
                    msvc,
                    append: Default::default(),
                    cache: None,
                };

                ctx.enable(TargetFeature {
                    check: "avx2",
                    defines: &["X86_AVX2"],
                    flags: &["-mavx2"],
                    msvc_flags: &["/arch:AVX2"],
                    files: &["chunkset_avx2", "compare256_avx2", "adler32_avx2"],
                });
                if ctx.enable(TargetFeature {
                    check: "sse2",
                    defines: &["X86_SSE2"],
                    flags: &["-msse2"],
                    msvc_flags: if is_64 { &[] } else { &["/arch:SSE2"] },
                    files: &["chunkset_sse2", "compare256_sse2", "slide_hash_sse2"],
                }) {
                    if arch != "x86_64" {
                        ctx.cfg.define("X86_NOCHECK_SSE2", None);
                    }
                }

                let sse3 = ctx.enable(TargetFeature {
                    check: "sse3",
                    defines: &["X86_SSSE3"],
                    flags: &["-msse3"],
                    msvc_flags: &["/arch:SSE3"],
                    files: &["adler32_ssse3", "chunkset_ssse3"],
                });

                let sse4 = ctx.enable(TargetFeature {
                    check: "sse4.2",
                    defines: &["X86_SSE42"],
                    flags: &["-msse4.2"],
                    msvc_flags: &["/arch:SSE4.2"],
                    files: &["adler32_sse42", "insert_string_sse42"],
                });

                let pclmulqdq = sse3
                    && sse4
                    && ctx.enable(TargetFeature {
                        check: "pclmulqdq",
                        defines: &["X86_PCLMULQDQ_CRC"],
                        flags: &["-mpclmul"],
                        msvc_flags: &[],
                        files: &["crc32_pclmulqdq"],
                    });

                ctx.enable(TargetFeature {
                    check: "xsave",
                    defines: &[],
                    flags: &["-mxsave"],
                    msvc_flags: &[],
                    files: &[],
                });

                pclmulqdq
            };

            if env::var_os("CARGO_FEATURE_ZLIB_NG_AVX512").is_some() {
                enable_avx512(&mut cfg, msvc, pclmulqdq);
            }
        }
        "aarch64" | "arm" => {
            let is_aarch64 = arch == "aarch64";

            cfg.define("ARM_FEATURES", None);
            cfg.file("src/zlib-ng/arch/arm/arm_features.c");

            let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

            let mut ctx = Ctx {
                cfg: &mut cfg,
                root: Some("arch/arm/"),
                enabled: Some(&target_features),
                msvc,
                append: Default::default(),
                cache: None,
            };

            // Support runtime detection on linux/android
            'caps: {
                if matches!(target_os.as_str(), "linux" | "android") {
                    ctx.append.defines.insert("HAVE_SYS_AUXV_H");

                    if is_aarch64 {
                        ctx.compile_check(
                            "arm",
                            TargetFeature {
                                check: "aarch64_caps",
                                defines: &["ARM_AUXV_HAS_CRC32"],
                                files: &[],
                                flags: &[],
                                msvc_flags: &[],
                            },
                        );
                        break 'caps;
                    }

                    if !ctx.compile_check(
                        "arm",
                        TargetFeature {
                            check: "arm_caps",
                            defines: &["ARM_AUXV_HAS_CRC32"],
                            files: &[],
                            flags: &[],
                            msvc_flags: &[],
                        },
                    ) {
                        ctx.compile_check(
                            "arm",
                            TargetFeature {
                                check: "arm_hwcaps",
                                defines: &["ARM_AUXV_HAS_CRC32", "ARM_ASM_HWCAP"],
                                files: &[],
                                flags: &[],
                                msvc_flags: &[],
                            },
                        );
                    }

                    if !ctx.compile_check(
                        "arm",
                        TargetFeature {
                            check: "arm_neon",
                            defines: &["ARM_AUXV_HAS_NEON"],
                            files: &[],
                            flags: &[],
                            msvc_flags: &[],
                        },
                    ) {
                        ctx.compile_check(
                            "arm",
                            TargetFeature {
                                check: "arm_hwneon",
                                defines: &["ARM_AUXV_HAS_NEON"],
                                files: &[],
                                flags: &[],
                                msvc_flags: &[],
                            },
                        );
                    }
                }
            }

            // According to the cmake macro, MSVC is missing the crc32 intrinsic
            // for arm, don't know if that is still true though
            if !msvc || is_aarch64 {
                ctx.enable(TargetFeature {
                    check: "crc",
                    defines: &["ARM_ACLE"],
                    files: &["crc32_acle", "insert_string_acle"],
                    flags: &["-march=armv8-a+crc"],
                    msvc_flags: &[],
                });
            }

            const NEON_64: &[&str] = &["-march=armv8-a+simd"];
            const NEON: &[&str] = &["-mfpu=neon"];

            let flags = if is_aarch64 { NEON_64 } else { NEON };

            if ctx.enable(TargetFeature {
                check: "neon",
                defines: &["ARM_NEON"],
                files: &[
                    "adler32_neon",
                    "chunkset_neon",
                    "compare256_neon",
                    "slide_hash_neon",
                ],
                flags,
                msvc_flags: &[],
            }) {
                if msvc {
                    ctx.append.defines.insert("__ARM_NEON__");
                }

                ctx.compile_check(
                    "arm",
                    TargetFeature {
                        check: "ld4",
                        defines: &["ARM_NEON_HASLD4"],
                        files: &[],
                        flags,
                        msvc_flags: &[],
                    },
                );
            }
        }
        _ => {
            // TODO: powerpc, riscv, s390
        }
    }

    let include = dst.join("include");

    fs::create_dir_all(&include).unwrap();

    let (zlib_h, mangle) = if compat {
        fs::copy("src/zlib-ng/zconf.h.in", include.join("zconf.h")).unwrap();
        ("zlib.h", "zlib_name_mangling.h")
    } else {
        fs::copy("src/zlib-ng/zconf-ng.h.in", include.join("zconf-ng.h")).unwrap();
        ("zlib-ng.h", "zlib_name_mangling-ng.h")
    };

    fs::copy(
        "src/zlib-ng/zlib_name_mangling.h.empty",
        include.join(mangle),
    )
    .unwrap();

    let version = {
        let mut version = None;
        strip_symbol_prefix(
            Path::new(&format!("src/zlib-ng/{zlib_h}.in")),
            &include.join(zlib_h),
            |line| {
                if line.contains("ZLIBNG_VERSION") && line.contains("#define") {
                    version = Some(line.split('"').nth(1).unwrap().to_owned());
                }
            },
        );

        version.expect("failed to detect ZLIBNG_VERSION")
    };

    cfg.include(include).include("src/zlib-ng");
    cfg.compile("z");

    fs::create_dir_all(lib.join("pkgconfig")).unwrap();
    fs::write(
        lib.join("pkgconfig/zlib.pc"),
        fs::read_to_string("src/zlib-ng/zlib.pc.in")
            .unwrap()
            .replace("@prefix@", dst.to_str().unwrap())
            .replace("@includedir@", "${prefix}/include")
            .replace("@libdir@", "${prefix}/lib")
            .replace("@VERSION@", &version),
    )
    .unwrap();

    let dst = dst.to_str().unwrap();
    println!("cargo:root={dst}");
    println!("cargo:rustc-link-search=native={}", lib.to_str().unwrap());
    println!("cargo:include={dst}/include");

    if !compat {
        println!("cargo:rustc-cfg=zng");
    }
}

/// Remove this once rustc stabilizes avx512 target features
///
/// <https://github.com/rust-lang/rust/issues/44839>
fn enable_avx512(cfg: &mut cc::Build, msvc: bool, with_pclmulqdq: bool) {
    const FEATURES: [TargetFeature; 4] = [
        TargetFeature {
            check: "basic",
            msvc_flags: &["/arch:AVX512"],
            flags: &["-mavx512f", "-mavx512dq", "-mavx512bw", "-mavx512vl"],
            defines: &["X86_AVX512"],
            files: &["adler32_avx512"],
        },
        TargetFeature {
            check: "mask",
            msvc_flags: &["/arch:AVX512"],
            flags: &["-mavx512f", "-mavx512dq", "-mavx512bw", "-mavx512vl"],
            defines: &["X86_MASK_INTRIN"],
            files: &[],
        },
        TargetFeature {
            check: "vnni",
            msvc_flags: &["/arch:AVX512"],
            flags: &[
                "-mavx512f",
                "-mavx512dq",
                "-mavx512bw",
                "-mavx512vl",
                "-mavx512vnni",
            ],
            defines: &["X86_AVX512VNNI"],
            files: &["adler32_avx512_vnni"],
        },
        TargetFeature {
            check: "vpclmulqdq",
            msvc_flags: &["/arch:AVX512"],
            flags: &["-mavx512f", "-mvpclmulqdq"],
            defines: &["X86_VPCLMULQDQ_CRC"],
            files: &["crc32_vpclmulqdq"],
        },
    ];

    let mut ctx = Ctx {
        cfg,
        root: Some("arch/x86/"),
        enabled: None,
        msvc,
        append: Default::default(),
        cache: None,
    };

    // The zlib-ng cmake scripts to check target features claim that GCC doesn't
    // generate good code unless mtune is set, not sure if this is still the
    // case, but we faithfully replicate it just in case
    if !msvc {
        for target in [&["-mtune=cascadelake"], &["-mtune=skylake-avx512"]] {
            if ctx.compile_check(
                "",
                TargetFeature {
                    check: "flag",
                    defines: &[],
                    flags: target,
                    msvc_flags: &[],
                    files: &[],
                },
            ) {
                break;
            }
        }
    }

    for tf in FEATURES {
        if tf.check == "vpclmulqdq" && !with_pclmulqdq {
            break;
        }

        ctx.compile_check("avx512", tf);
    }
}

#[allow(dead_code)]
fn main() {
    let target = env::var("TARGET").unwrap();
    build_zlib_ng(&target, false);
}
