use std::env::args_os;
use std::error::Error as ErrorTrait;
use std::fmt;
use std::io::{Read, BufRead, BufReader};
use std::io::{Write, stderr};
use std::io::Result as IOResult;
use std::io::Error as IOError;
use std::fs::{File, OpenOptions, create_dir, metadata};
use std::path::{Path, PathBuf};
use std::process::exit;

extern crate getopts;

extern crate datetime;
use datetime::{LocalDateTime, ISO};

extern crate zoneinfo_parse;
use zoneinfo_parse::line::{Line};
use zoneinfo_parse::table::{Table, TableBuilder};
use zoneinfo_parse::structure::{Structure, Child};
use zoneinfo_parse::transitions::{TableTransitions};

#[macro_use]
extern crate quick_error;

#[macro_use]
mod util;


fn main() {
    if let Err(e) = build_data_crate() {
        println_stderr!("{}", e);
        exit(1);
    }
}

fn build_data_crate() -> Result<(), SomeError> {
    let mut opts = getopts::Options::new();
    opts.reqopt("o", "output", "directory to write the crate into", "DIR");

    let matches = try!(opts.parse(args_os().skip(1)));
    let data_crate = try!(DataCrate::new(matches.opt_str("output").unwrap(), &matches.free));
    try!(data_crate.run());

    println!("All done.");
    Ok(())
}

struct DataCrate {
    base_path: PathBuf,
    table: Table,
}


quick_error! {
    #[derive(Debug)]
    enum SomeError {
        IO(err: IOError) {
            from()
            display(x) -> ("IO error: {}", err)
        }

        Errors(errs: Errors) {
            from()
            display(x) -> ("{}", errs)
        }

        Getopts(err: getopts::Fail) {
            from()
            display(x) -> ("Error parsing options: {}", err)
        }
    }
}


impl DataCrate {

    fn new<P>(base_path: P, input_file_paths: &[String]) -> Result<DataCrate, Errors>
    where P: Into<PathBuf> {

        let mut builder = TableBuilder::new();
        let mut errors = Vec::new();

        for arg in input_file_paths {
            let f = File::open(arg).unwrap();
            let reader = BufReader::new(f);

            for (line_number, line) in reader.lines().enumerate() {
                let line = line.unwrap();

                // Strip out the comment portion from the line, if any.
                let line_portion = match line.find('#') {
                    Some(pos) => &line[..pos],
                    None      => &line[..],
                };

                let result = match Line::from_str(line_portion) {

                    // If there’s an error, then display which line failed to parse.
                    Err(e) => {
                        let error = ParseError {
                            filename: arg.clone(),
                            line: line_number + 1,
                            error: e.description().to_owned(),
                        };

                        errors.push(error);
                        continue;
                    },

                    // Ignore any spaces
                    Ok(Line::Space) => { continue },

                    Ok(Line::Rule(rule))         => builder.add_rule_line(rule),
                    Ok(Line::Link(link))         => builder.add_link_line(link),
                    Ok(Line::Zone(zone))         => builder.add_zone_line(zone),
                    Ok(Line::Continuation(cont)) => builder.add_continuation_line(cont),
                };

                if let Err(e) = result {
                    let error = ParseError {
                        filename: arg.clone(),
                        line: line_number + 1,
                        error: e.description().to_owned(),
                    };

                    errors.push(error);
                }
            }
        }

        if errors.is_empty() {
            Ok(DataCrate {
                base_path: base_path.into(),
                table: builder.build()
            })
        }
        else {
            Err(Errors(errors))
        }
    }

    fn run(&self) -> IOResult<()> {
        try!(self.create_structure_directories());
        try!(self.write_zonesets());
        Ok(())
    }

    fn create_structure_directories(&self) -> IOResult<()> {
        let base_mod_path = self.base_path.join("mod.rs");
        let mut base_w = try!(OpenOptions::new().write(true).create(true).truncate(true).open(base_mod_path));
        try!(writeln!(base_w, "{}", WARNING_HEADER));
        try!(writeln!(base_w, "{}", MOD_HEADER));

        for entry in self.table.structure() {
            if !entry.name.contains('/') {
                try!(writeln!(base_w, "pub mod {};", entry.name));
            }

            let components: PathBuf = entry.name.split('/').collect();
            let dir_path = self.base_path.join(components);
            if !is_directory(&dir_path) {
                println!("Creating directory {:?}", &dir_path);
                try!(create_dir(&dir_path));
            }

            let mod_path = dir_path.join("mod.rs");
            let mut w = try!(OpenOptions::new().write(true).create(true).truncate(true).open(mod_path));
            for child in &entry.children {
                match *child {
                    Child::TimeZone(ref name) => {
                        let sanichild = sanitise_name(name);
                        try!(writeln!(w, "mod {};", sanichild));
                        try!(writeln!(w, "pub use self::{}::ZONE as {};\n", sanichild, sanichild));
                    },
                    Child::Submodule(ref name) => {
                        let sanichild = sanitise_name(name);
                        try!(writeln!(w, "pub mod {};\n", sanichild));
                    },
                }
            }
        }

        let mut keys: Vec<_> = self.table.zonesets.keys().chain(self.table.links.keys()).collect();
        keys.sort();

        try!(writeln!(base_w, "\n\n"));
        for name in keys.iter().filter(|f| !f.contains('/')) {
            let sanichild = sanitise_name(name);
            try!(writeln!(base_w, "mod {};", sanichild));
            try!(writeln!(base_w, "pub use self::{}::ZONE as {};\n", sanichild, sanichild));
        }

        try!(writeln!(base_w, "\n\n"));
        try!(writeln!(base_w, "pub fn lookup(input: &str) -> Option<&'static StaticTimeZone<'static>> {{"));
        for name in &keys {
            try!(writeln!(base_w, "    if input == {:?} {{", name));
            try!(writeln!(base_w, "        return Some(&{});", sanitise_name(name).replace("/", "::")));
            try!(writeln!(base_w, "    }}"));
        }
        try!(writeln!(base_w, "    return None;"));
        try!(writeln!(base_w, "}}"));

        Ok(())
    }

    fn write_zonesets(&self) -> IOResult<()> {
        for name in self.table.zonesets.keys().chain(self.table.links.keys()) {
            let components: PathBuf = name.split('/').map(sanitise_name).collect();
            let zoneset_path = self.base_path.join(components).with_extension("rs");
            let mut w = try!(OpenOptions::new().write(true).create(true).truncate(true).open(zoneset_path));
            try!(writeln!(w, "{}", WARNING_HEADER));
            try!(writeln!(w, "{}", ZONEINFO_HEADER));

            try!(writeln!(w, "pub static ZONE: StaticTimeZone<'static> = StaticTimeZone {{"));
            try!(writeln!(w, "    name: {:?},", name));
            try!(writeln!(w, "    fixed_timespans: FixedTimespanSet {{"));

            let set = self.table.timespans(&*name);

            try!(writeln!(w, "        first: FixedTimespan {{"));
            try!(writeln!(w, "            offset: {:?},  // UTC offset {:?}, DST offset {:?}", set.first.total_offset(), set.first.utc_offset, set.first.dst_offset));
            try!(writeln!(w, "            is_dst: {:?},", set.first.dst_offset != 0));
            try!(writeln!(w, "            name:   Cow::Borrowed({:?}),", set.first.name));
            try!(writeln!(w, "        }},"));

            try!(writeln!(w, "        rest: &["));

            for t in &set.rest {
                try!(writeln!(w, "        ({:?}, FixedTimespan {{  // {} UTC", t.0, LocalDateTime::at(t.0).iso()));

                // Write the total offset (the only value that gets used)
                // and both the offsets that get added together, as a
                // comment in the data crate.
                try!(writeln!(w, "            offset: {:?},  // UTC offset {:?}, DST offset {:?}", t.1.total_offset(), t.1.utc_offset, t.1.dst_offset));
                try!(writeln!(w, "            is_dst: {:?},", t.1.dst_offset != 0));
                try!(writeln!(w, "            name:   Cow::Borrowed({:?}),", t.1.name));
                try!(writeln!(w, "        }}),"));
            }
            try!(writeln!(w, "    ]}},"));
            try!(writeln!(w, "}};\n\n"));
        }

        Ok(())
    }
}

fn is_directory(path: &Path) -> bool {
    match metadata(path) {
        Ok(m)  => m.is_dir(),
        Err(_) => false,
    }
}

fn sanitise_name(name: &str) -> String {
    name.replace("-", "_")
}

#[derive(Debug)]
struct ParseError {
    filename: String,
    line: usize,
    error: String,
}

#[derive(Debug)]
struct Errors(Vec<ParseError>);

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        for err in &self.0 {
            try!(write!(f, "{}:{}: {}\n", err.filename, err.line, err.error));
        }
        Ok(())
    }
}

const WARNING_HEADER: &'static str = r##"
// ------
// This file is autogenerated!
// Any changes you make may be overwritten.
// ------
"##;

const ZONEINFO_HEADER: &'static str = r##"
use std::borrow::Cow;
use datetime::zone::{StaticTimeZone, FixedTimespanSet, FixedTimespan};
"##;

const MOD_HEADER: &'static str = r##"
use datetime::zone::StaticTimeZone;
"##;
