#!/usr/bin/env bash
# Verify that LIBZ_SYS_STATIC=0 overrides the static feature and selects system libz
# without compiling or emitting metadata for the bundled library.
set -euo pipefail

target_dir=target/force-system-libz

CC=false LIBZ_SYS_STATIC=0 CARGO_TARGET_DIR="$target_dir" \
    cargo test --features static --no-run

build_output=$(find "$target_dir/debug/build" -path '*/libz-sys-*/output' -print -quit)
if [[ -z "$build_output" ]]; then
    echo 'error: libz-sys build-script output was not found' >&2
    exit 1
fi

if ! grep -Fxq 'cargo:rustc-link-lib=z' "$build_output"; then
    echo "error: $build_output does not request system libz" >&2
    exit 1
fi

if grep -Fxq 'cargo:rustc-link-lib=static=z' "$build_output"; then
    echo "error: $build_output requests static libz" >&2
    exit 1
fi

archive=$(find "$target_dir" -name libz.a -print -quit)
if [[ -n "$archive" ]]; then
    echo "error: bundled libz archive was produced at $archive" >&2
    exit 1
fi

echo 'LIBZ_SYS_STATIC=0 linked system libz without building bundled libz.a'
