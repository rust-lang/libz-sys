#![allow(bad_style, improper_ctypes)]

use libc::*;
#[cfg(zng)]
use libz_ng_sys::*;
#[cfg(not(zng))]
use libz_sys::*;

mod generated {
    #![allow(
        missing_abi,
        function_casts_as_integer,
        clippy::all
    )]

    use super::*;

    include!(concat!(env!("OUT_DIR"), "/all.rs"));

    pub(crate) fn run() {
        main();
    }
}

fn main() {
    generated::run();
}
