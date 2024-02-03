#!/usr/bin/env bash
# Script for building your rust projects.
set -e

required_arg() {
    if [ -z "$1" ]; then
        echo "Required argument $2 missing"
        exit 1
    fi
}

# $1 {path} = Path to cross/cargo executable
CROSS=$1
# $2 {string} = <Target Triple>
TARGET_TRIPLE=$2

required_arg $CROSS 'CROSS'
required_arg $TARGET_TRIPLE '<Target Triple>'

if [ "${TARGET_TRIPLE%-windows-gnu}" != "$TARGET_TRIPLE" ]; then
    # On windows-gnu targets, we need to set the PATH to include MinGW
    if [ "${TARGET_TRIPLE#x86_64-}" != "$TARGET_TRIPLE" ]; then
        PATH=/c/msys64/mingw64/bin:/c/msys64/usr/bin:$PATH
    elif [ "${TARGET_TRIPLE#i?86-}" != "$TARGET_TRIPLE" ]; then
        PATH=/c/msys64/mingw32/bin:/c/msys64/usr/bin:$PATH
    else
        echo Unknown windows-gnu target
        exit 1
    fi
fi

$CROSS test --target $TARGET_TRIPLE
$CROSS run --target $TARGET_TRIPLE --manifest-path systest/Cargo.toml

echo '::group::=== zlib-ng build ==='
$CROSS test --target $TARGET_TRIPLE --no-default-features --features zlib-ng
$CROSS run --target $TARGET_TRIPLE --manifest-path systest/Cargo.toml --no-default-features --features zlib-ng
echo '::endgroup::'

# Note we skip compiling these targets on CI because the gcc version currently used in
# cross for them is 5.4, ~8 years old at this point, hopefully it will be updated...sometime
skip_triples=("x86_64-unknown-linux-gnu" "i686-unknown-linux-gnu" "aarch64-unknown-linux-gnu" "arm-unknown-linux-gnueabihf" "s390x-unknown-linux-gnu")
if [[ -z $CI ]] || ! [[ ${skip_triples[@]} =~ "${TARGET_TRIPLE}" ]]; then
    echo '::group::=== zlib-ng-no-cmake-experimental-community-maintained build ==='

    $CROSS test --target "$TARGET_TRIPLE" --no-default-features --features zlib-ng-no-cmake-experimental-community-maintained || echo "::warning file=$(basename $0),line=$LINENO::Failed to test zlib-ng with --features zlib-ng-no-cmake-experimental-community-maintained"
    $CROSS run --target "$TARGET_TRIPLE" --manifest-path systest/Cargo.toml --no-default-features --features zlib-ng-no-cmake-experimental-community-maintained || echo "::warning file=$(basename $0),line=$LINENO::Failed to run systest with --features zlib-ng-no-cmake-experimental-community-maintained"

    echo '::endgroup::'
fi

echo '::group::=== libz-ng-sys build ==='
mv Cargo-zng.toml Cargo.toml
mv systest/Cargo-zng.toml systest/Cargo.toml
$CROSS test --target $TARGET_TRIPLE
$CROSS run --target $TARGET_TRIPLE --manifest-path systest/Cargo.toml
echo '::endgroup::'

echo '::group::=== flate2 validation ==='
git clone https://github.com/rust-lang/flate2-rs flate2
git worktree add flate2/libz-sys
git worktree add flate2/libz-ng-sys

cd flate2
(cd libz-sys
  git submodule update --init
)
(cd libz-ng-sys
  git submodule update --init
  mv systest/Cargo-zng.toml systest/Cargo.toml
  mv Cargo-zng.toml Cargo.toml
)

echo "[workspace]" >> Cargo.toml
mkdir .cargo
cat <<EOF >.cargo/config.toml
[patch."crates-io"]
libz-sys = { path = "./libz-sys" }
libz-ng-sys = { path = "./libz-ng-sys" }
EOF

set -x
$CROSS test --features zlib --target $TARGET_TRIPLE
$CROSS test --features zlib-default --no-default-features --target $TARGET_TRIPLE
$CROSS test --features zlib-ng --no-default-features --target $TARGET_TRIPLE
$CROSS test --features zlib-ng-compat --no-default-features --target $TARGET_TRIPLE
echo '::endgroup::'