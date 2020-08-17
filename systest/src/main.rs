#![allow(bad_style, improper_ctypes)]

extern crate libc;
extern crate libz_sys;

use libc::*;
use libz_sys::*;

include!(concat!(env!("OUT_DIR"), "/all.rs"));
