#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(improper_ctypes)]

use std::io::Result;
use std::path::Path;

pub mod libfat32 {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub fn mount<'a, Dev: AsRef<Path>, Dest: AsRef<Path>, Args: AsRef<[&'a str]>>(path: Dev, dest: Dest, args: Args) -> Result<()> {
    todo!()
}