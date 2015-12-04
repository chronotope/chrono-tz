//! Parsing Olson DB formats.

#![crate_name = "zoneinfo_parse"]
#![crate_type = "rlib"]
#![crate_type = "dylib"]

#![warn(missing_copy_implementations)]
#![warn(missing_debug_implementations)]
//#![warn(missing_docs)]

#![warn(trivial_casts, trivial_numeric_casts)]
#![warn(unused_qualifications)]
#![warn(unused_results)]

extern crate datetime;
extern crate regex;
#[macro_use] extern crate lazy_static;

pub mod line;
pub mod table;
pub mod transitions;
pub mod structure;
