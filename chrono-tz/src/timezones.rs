#![allow(non_camel_case_types, clippy::unreadable_literal)]

#[cfg(feature = "filter-by-regex")]
include!(concat!(env!("OUT_DIR"), "/timezones.rs"));

#[cfg(not(feature = "filter-by-regex"))]
include!("prebuilt_timezones.rs");
