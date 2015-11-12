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
pub use line::Line;

mod table;
pub use table::{Table, TableBuilder, Saving, FixedTimespanSet, FixedTimespan};

mod structure;
pub use structure::*;