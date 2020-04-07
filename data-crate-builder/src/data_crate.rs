//! Creating the data crate from several input files, and the writing of Rust
//! files afterwards.

use std::error::Error as ErrorTrait;
use std::io::{Read, BufRead, BufReader};
use std::io::Write;
use std::io::Result as IOResult;
use std::fs::{File, OpenOptions, create_dir};
use std::path::PathBuf;

use datetime::{LocalDateTime, ISO};

use zoneinfo_parse::line::{Line};
use zoneinfo_parse::table::{Table, TableBuilder};
use zoneinfo_parse::structure::{Structure, Child};
use zoneinfo_parse::transitions::{TableTransitions};

use phf_codegen::Map as PHFMap;

use errors::{Error, ParseError};


/// The entire contents of some zoneinfo data files.
pub struct DataCrate {

    /// The base path to write the Rust files to.
    base_path: PathBuf,

    /// The data to write.
    table: Table,
}

impl DataCrate {

    /// Creates a new data crate based on the contents of several files,
    /// returning an error if any of the files can’t be opened or any of the
    /// lines doesn’t parse correctly. The resulting data crate value can then
    /// be turned into many Rust files of time zone info.
    ///
    /// All the errors are stored and returned in one go, rather than
    /// returning early after the first one.
    pub fn new<P>(base_path: P, input_file_paths: &[String]) -> Result<DataCrate, Error>
    where P: Into<PathBuf> {

        let mut builder = TableBuilder::new();
        let mut errors = Vec::new();

        for arg in input_file_paths {
            let f = File::open(arg)?;
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

        // If there are *any* errors, then we can’t return success.
        if errors.is_empty() {
            Ok(DataCrate {
                base_path: base_path.into(),
                table: builder.build()
            })
        }
        else {
            Err(errors.into())
        }
    }

    /// There are two steps to writing the data: creating the directories the
    /// data goes in (and the `mod.rs` files for those directories), and then
    /// creating the files inside those directories.
    pub fn run(&self) -> IOResult<()> {
        self.create_structure_directories()?;
        self.write_zonesets()?;
        Ok(())
    }

    /// Creates the directories that the Rust files get written to later. Also
    /// creates `mod.rs` files inside those directories.
    fn create_structure_directories(&self) -> IOResult<()> {
        let mut open_opts = OpenOptions::new();
        open_opts.write(true).create(true).truncate(true);

        let base_mod_path = self.base_path.join("mod.rs");
        let mut base_w = open_opts.open(base_mod_path)?;

        writeln!(base_w, "{}", WARNING_HEADER)?;
        writeln!(base_w, "{}", MOD_HEADER)?;

        for entry in self.table.structure() {
            if !entry.name.contains('/') {
                writeln!(base_w, "pub mod {};", entry.name)?;
            }

            let components: PathBuf = entry.name.split('/').collect();
            let dir_path = self.base_path.join(components);
            if !dir_path.is_dir() {
                println!("Creating directory {:?}", &dir_path);
                create_dir(&dir_path)?;
            }

            let mod_path = dir_path.join("mod.rs");
            let mut w = open_opts.open(mod_path)?;
            for child in &entry.children {
                match *child {
                    Child::TimeZone(ref name) => {
                        let sanichild = sanitise_name(name);
                        writeln!(w, "mod {};", sanichild)?;
                        writeln!(w, "pub use self::{}::ZONE as {};\n", sanichild, sanichild)?;
                    },
                    Child::Submodule(ref name) => {
                        let sanichild = sanitise_name(name);
                        writeln!(w, "pub mod {};\n", sanichild)?;
                    },
                }
            }
        }

        let mut keys: Vec<_> = self.table.zonesets.keys().chain(self.table.links.keys()).collect();
        keys.sort();

        writeln!(base_w, "\n\n")?;
        for name in keys.iter().filter(|f| !f.contains('/')) {
            let sanichild = sanitise_name(name);
            writeln!(base_w, "mod {};", sanichild)?;
            writeln!(base_w, "pub use self::{}::ZONE as {};\n", sanichild, sanichild)?;
        }

        writeln!(base_w, "\n\n")?;
        write!(base_w, "static ZONES: phf::Map<&'static str, &'static StaticTimeZone<'static>> = ")?;

        let mut phf_map = PHFMap::new();
        for name in &keys {
            phf_map.entry(&***name, &format!("&{}", sanitise_name(name).replace("/", "::")));
        }
        phf_map.build(&mut base_w)?;

        writeln!(base_w, ";\n\npub fn lookup(input: &str) -> Option<&'static StaticTimeZone<'static>> {{")?;
        writeln!(base_w, "    ZONES.get(input).cloned()")?;
        writeln!(base_w, "}}")?;

        Ok(())
    }

    /// Writes each zone file as a Rust file.
    fn write_zonesets(&self) -> IOResult<()> {
        for name in self.table.zonesets.keys().chain(self.table.links.keys()) {
            let components: PathBuf = name.split('/').map(sanitise_name).collect();
            let zoneset_path = self.base_path.join(components).with_extension("rs");
            let mut w = OpenOptions::new().write(true).create(true).truncate(true).open(zoneset_path);
            writeln!(w, "{}", WARNING_HEADER)?;
            writeln!(w, "{}", ZONEINFO_HEADER)?;

            writeln!(w, "pub static ZONE: StaticTimeZone<'static> = StaticTimeZone {{")?;
            writeln!(w, "    name: {:?},", name)?;
            writeln!(w, "    fixed_timespans: FixedTimespanSet {{")?;

            let set = self.table.timespans(&*name).unwrap();

            writeln!(w, "        first: FixedTimespan {{")?;
            writeln!(w, "            offset: {:?},  // UTC offset {:?}, DST offset {:?}", set.first.total_offset(), set.first.utc_offset, set.first.dst_offset)?;
            writeln!(w, "            is_dst: {:?},", set.first.dst_offset != 0)?;
            writeln!(w, "            name:   Cow::Borrowed({:?}),", set.first.name)?;
            writeln!(w, "        }},")?;

            writeln!(w, "        rest: &[")?;

            for t in &set.rest {
                writeln!(w, "        ({:?}, FixedTimespan {{  // {} UTC", t.0, LocalDateTime::at(t.0).iso())?;

                // Write the total offset (the only value that gets used)
                // and both the offsets that get added together, as a
                // comment in the data crate.
                writeln!(w, "            offset: {:?},  // UTC offset {:?}, DST offset {:?}", t.1.total_offset(), t.1.utc_offset, t.1.dst_offset)?;
                writeln!(w, "            is_dst: {:?},", t.1.dst_offset != 0)?;
                writeln!(w, "            name:   Cow::Borrowed({:?}),", t.1.name)?;
                writeln!(w, "        }}),")?;
            }
            writeln!(w, "    ]}},")?;

            writeln!(w, "}};\n\n")?;
        }

        Ok(())
    }
}

/// Rust places constraints on what modules can be named, so we need to
/// “sanitise” some of the time zone names before they can be made into
/// modules.
fn sanitise_name(name: &str) -> String {
    name.replace("-", "_")
}


/// The comment placed at the top of all autogenerated files, so they aren’t
/// ever changed by a human and then overwritten by this program later.
const WARNING_HEADER: &'static str = r##"
// ------
// This file is autogenerated!
// Any changes you make may be overwritten.
// ------
"##;

/// The imports needed for a zoneinfo Rust file.
const ZONEINFO_HEADER: &'static str = r##"
use std::borrow::Cow;
use datetime::zone::{StaticTimeZone, FixedTimespanSet, FixedTimespan};
"##;

/// The imports needed for a `mod.rs` file.
const MOD_HEADER: &'static str = r##"
use datetime::zone::StaticTimeZone;
use phf;
"##;
