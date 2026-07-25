#![cfg(any(target_os = "wasi", target_os = "emscripten"))]

#[test]
fn compress_roundtrip() {
    let input = b"libz-sys on WebAssembly";
    let mut compressed = vec![0; unsafe { libz_sys::compressBound(input.len() as _) } as usize];
    let mut compressed_len = compressed.len() as _;

    assert_eq!(
        unsafe {
            libz_sys::compress(
                compressed.as_mut_ptr(),
                &mut compressed_len,
                input.as_ptr(),
                input.len() as _,
            )
        },
        libz_sys::Z_OK
    );

    let mut output = vec![0; input.len()];
    let mut output_len = output.len() as _;
    assert_eq!(
        unsafe {
            libz_sys::uncompress(
                output.as_mut_ptr(),
                &mut output_len,
                compressed.as_ptr(),
                compressed_len,
            )
        },
        libz_sys::Z_OK
    );
    assert_eq!(&output[..output_len as usize], input);
}
