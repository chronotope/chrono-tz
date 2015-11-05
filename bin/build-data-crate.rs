extern crate zoneinfo_parse;
use zoneinfo_parse::{Line, TableBuilder, Table, Structure, Child};

use std::env::args;
use std::io::prelude::*;
use std::io::BufReader;
use std::io::Result as IoResult;
use std::fs::{File, OpenOptions, create_dir, metadata};
use std::path::{Path, PathBuf};
use std::process::exit;


fn main() {
    let args: Vec<_> = args().skip(1).collect();

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
        try!(self.create_structure_directories());
        try!(self.write_zonesets());
        Ok(())
    }

    fn create_structure_directories(&self) -> IoResult<()> {
        let base_mod_path = self.base_path.join("mod.rs");
        let mut base_w = try!(OpenOptions::new().write(true).create(true).truncate(true).open(base_mod_path));
        try!(writeln!(base_w, "{}", WARNING_HEADER));
        try!(writeln!(base_w, "{}", ZONEINFO_HEADER));

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

        let mut keys: Vec<_> = self.table.zonesets.keys().collect();
        keys.sort();

        try!(writeln!(base_w, "\n\n"));
        for name in keys.iter().filter(|f| !f.contains('/')) {
            let sanichild = sanitise_name(name);
            try!(writeln!(base_w, "mod {};", sanichild));
            try!(writeln!(base_w, "pub use self::{}::ZONE as {};\n", sanichild, sanichild));
        }

        try!(writeln!(base_w, "\n\n"));
        try!(writeln!(base_w, "pub fn lookup(input: &str) -> Option<Zone> {{"));
        for name in &keys {
            try!(writeln!(base_w, "    if input == {:?} {{", name));
            try!(writeln!(base_w, "        return Some({});", sanitise_name(name).replace("/", "::")));
            try!(writeln!(base_w, "    }}"));
        }
        try!(writeln!(base_w, "    return None;"));
        try!(writeln!(base_w, "}}"));

        Ok(())
    }

    fn write_zonesets(&self) -> IoResult<()> {
        for name in self.table.zonesets.keys() {
            let components: PathBuf = name.split('/').map(sanitise_name).collect();
            let zoneset_path = self.base_path.join(components).with_extension("rs");
            let mut w = try!(OpenOptions::new().write(true).create(true).truncate(true).open(zoneset_path));
            try!(writeln!(w, "{}", WARNING_HEADER));
            try!(writeln!(w, "{}", ZONEINFO_HEADER));

            try!(writeln!(w, "pub const ZONE: Zone<'static> = Zone {{"));
            try!(writeln!(w, "    name: {:?},", name));
            try!(writeln!(w, "    transitions: &["));

            let set = self.table.transitions(&*name);

                try!(writeln!(w, "        Transition {{"));
                try!(writeln!(w, "            occurs_at: None,"));
                try!(writeln!(w, "            offset: {:?},  // UTC offset {:?}, DST offset {:?}", set.first.total_offset(), set.first.utc_offset, set.first.dst_offset));
                try!(writeln!(w, "            name: {:?},", set.first.name));
                try!(writeln!(w, "        }},"));

            for t in &set.rest {
                try!(writeln!(w, "        Transition {{"));
                try!(writeln!(w, "            occurs_at: {:?},", Some(t.0)));

                // Write the total offset (the only value that gets used)
                // and both the offsets that get added together, as a
                // comment in the data crate.
                try!(writeln!(w, "            offset: {:?},  // UTC offset {:?}, DST offset {:?}", t.1.total_offset(), t.1.utc_offset, t.1.dst_offset));
                try!(writeln!(w, "            name: {:?},", t.1.name));
                try!(writeln!(w, "        }},"));
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

const ZONEINFO_HEADER: &'static str = r##"
use datetime::zoned::zoneinfo::*;
"##;
