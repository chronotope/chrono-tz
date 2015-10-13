//! Parsing Olson DB formats.

#![warn(missing_copy_implementations)]
#![warn(missing_debug_implementations)]
//#![warn(missing_docs)]
#![warn(trivial_casts, trivial_numeric_casts)]
#![warn(unused_qualifications)]
#![warn(unused_results)]

extern crate datetime;
extern crate regex;
#[macro_use] extern crate lazy_static;

mod line;
pub use line::{Line, ZoneTime};
// ^ ZoneTime needs to be here, otherwise you get a linker error (weird!)

mod table;
pub use table::{Table, TableBuilder, Saving};

mod structure;
pub use structure::*;