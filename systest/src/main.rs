#![allow(bad_style, improper_ctypes)]

use libc::*;
#[cfg(not(zng))]
use libz_sys::*;
#[cfg(zng)]
use libz_ng_sys::*;

include!(concat!(env!("OUT_DIR"), "/all.rs"));
