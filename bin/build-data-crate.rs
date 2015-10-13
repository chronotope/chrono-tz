extern crate zoneinfo_parse;
use zoneinfo_parse::{Line, TableBuilder, Table, Structure, Child, Saving, ZoneTime};

use std::env::args;
use std::io::prelude::*;
use std::io::BufReader;
use std::io::Result as IoResult;
use std::fs::{File, OpenOptions, create_dir, metadata};
use std::path::{Path, PathBuf};
use std::process::exit;

fn main() {
    let args: Vec<_> = args().skip(1).collect();

    // let base_path = "~/Code/datetime/zoneinfo-data/src/data";
    let data_crate = match DataCrate::new(&args[0], &args[1..]) {
        Ok(dc) => dc,
        Err(_) => {
            println!("Errors occurred - not going any further.");
            exit(1);
        },
    };

    match data_crate.run() {
        Ok(()) => println!("All done."),
        Err(e) => {
            println!("IO error: {}", e);
            exit(1);
        },
    }
}

struct DataCrate {
    base_path: PathBuf,
    table: Table,
}

impl DataCrate {

    fn new<P>(base_path: P, input_file_paths: &[String]) -> Result<DataCrate, u32>
    where P: Into<PathBuf> {

        let mut builder = TableBuilder::new();
        let mut errors = 0;

        for arg in input_file_paths {
            let f = File::open(arg).unwrap();
            let reader = BufReader::new(f);

            for line in reader.lines() {
                let line = line.unwrap();

                // Strip out the comment portion from the line, if any.
                let line_portion = match line.find('#') {
                    Some(pos) => &line[..pos],
                    None      => &line[..],
                };

                let result = match Line::from_str(line_portion) {

                    // If there's an error, then display which line failed to parse.
                    Err(_) => {
                        println!("Failed to parse line: {:?}", line_portion);
                        errors += 1;
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
                    errors += 1;
                    println!("Error: {:?}", e);
                }
            }
        }

        if errors == 0 {
            Ok(DataCrate {
                base_path: base_path.into(),
                table: builder.build()
            })
        }
        else {
            Err(errors)
        }
    }

    fn run(&self) -> IoResult<()> {
        try!(self.write_rulesets());
        try!(self.create_structure_directories());
        try!(self.write_zonesets());
        Ok(())
    }

    fn write_rulesets(&self) -> IoResult<()> {
        let path = self.base_path.join("rulesets.rs");
        let mut w = try!(OpenOptions::new().write(true).create(true).truncate(true).open(path));
        try!(writeln!(w, "{}", WARNING_HEADER));
        try!(writeln!(w, "{}", RULESETS_HEADER));

        for (name, ruleset) in &self.table.rulesets {
            try!(writeln!(w, "pub const {}: Ruleset<'static> = Ruleset {{ rules: &[", name.replace("-", "_")));
            for rule in &ruleset.0 {
                try!(writeln!(w, "    {:?},", rule));
            }
            try!(writeln!(w, "] }};\n\n"));
        }

        Ok(())
    }

    fn create_structure_directories(&self) -> IoResult<()> {
        let base_mod_path = self.base_path.join("mod.rs");
        let mut base_w = try!(OpenOptions::new().write(true).create(true).truncate(true).open(base_mod_path));
        try!(write!(base_w, "pub mod rulesets;\n"));

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
                    Child::Ruleset(ref name) => {
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

        Ok(())
    }

    fn write_zonesets(&self) -> IoResult<()> {
        for (name, zoneset) in &self.table.zonesets {
            let components: PathBuf = name.split('/').map(sanitise_name).collect();
            let zoneset_path = self.base_path.join(components).with_extension("rs");
            let mut w = try!(OpenOptions::new().write(true).create(true).truncate(true).open(zoneset_path));
            try!(writeln!(w, "{}", WARNING_HEADER));
            try!(writeln!(w, "{}", ZONESETS_HEADER));

            try!(writeln!(w, "pub const ZONE: Zone<'static> = Zone {{"));
            try!(writeln!(w, "    name: {:?},", name));
            try!(writeln!(w, "    timespans: &["));

            let mut last_end_time: Option<ZoneTime> = None;
            for info in &zoneset.0 {
                let saving = match info.saving {
                    Saving::NoSaving         => "Saving::NoSaving".to_owned(),
                    Saving::OneOff(by)       => format!("Saving::OneOff({})", by),
                    Saving::Multiple(ref n)  => format!("Saving::Multiple(&rulesets::{})", sanitise_name(n)),
                };

                try!(writeln!(w, "        Timespan {{"));
                try!(writeln!(w, "            offset: {:?},", info.gmt_offset));
                try!(writeln!(w, "            format: {:?},", info.format));
                try!(writeln!(w, "            saving: {},", saving));
                try!(writeln!(w, "            start_time: {:?},", last_end_time.map(|s| s.to_timestamp())));
                try!(writeln!(w, "            end_time:   {:?},", info.until.map(|s| s.to_timestamp())));
                try!(writeln!(w, "        }},"));

                last_end_time = info.until;
            }
            try!(writeln!(w, "    ],"));
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

const WARNING_HEADER: &'static str = r##"
// ------
// This file is autogenerated!
// Any changes you make may be overwritten.
// ------
"##;

const RULESETS_HEADER: &'static str = r##"
use code::*;
use code::YearSpec::*;
use code::DaySpec::*;
use datetime::local::Month::*;
use datetime::local::Weekday::*;
"##;

const ZONESETS_HEADER: &'static str = r##"
use code::*;
use code::Saving::*;

#[allow(unused_imports)]
use data::rulesets;
"##;
