#![allow(bad_style, improper_ctypes)]

extern crate libz_sys;
extern crate libc;

use libc::*;
use libz_sys::*;

include!(concat!(env!("OUT_DIR"), "/all.rs"));
