# Data crate builder

This is a little program that reads the contents of one or more [zoneinfo files](https://github.com/eggert/tz) and outputs Rust code that contains parsed structs of the data within those files.

It's used to create the [zoneinfo-data](https://github.com/rust-datetime/zoneinfo-data) crate, but can also be used to generate custom versions of that crate if you want to deal with your own time zone data.


## Usage

To build your own crate, run the program with the output directory as the `--option` argument, and the rest of the files as unnamed arguments. For example:

    cargo run -- --output ~/my-crate ~/tz/africa ~/tz/antarctica ~/tz/asia ...

This will place all the Rust code within `~/my-crate`. The directory will have to be created first.
