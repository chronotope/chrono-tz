#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]

#[cfg(feature = "filter-by-regex")]
include!(concat!(env!("OUT_DIR"), "/directory.rs"));

#[cfg(not(feature = "filter-by-regex"))]
include!("prebuilt/directory.rs");
