# libz-sys

A common library for linking `libz` to rust programs (also known as zlib).

[Documentation](https://docs.rs/libz-sys)

# High-level API

This crate provides bindings to the raw low-level C API. For a higher-level
safe API to work with DEFLATE, zlib, or gzip streams, see
[`flate2`](https://docs.rs/flate2). `flate2` also supports alternative
implementations, including slower but pure Rust implementations.

# License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in `libz-sys` by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
