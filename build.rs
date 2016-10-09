extern crate zoneinfo_parse;

use zoneinfo_parse::line::Line;
use zoneinfo_parse::table::{TableBuilder, Table};
use zoneinfo_parse::transitions::TableTransitions;
use zoneinfo_parse::structure::{Structure, Child};
use zoneinfo_parse::transitions::FixedTimespan;

use std::env;
use std::path::Path;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::collections::HashSet;

// This function is needed until zoneinfo_parse handles comments correctly.
// Technically a '#' symbol could occur between double quotes and should be
// ignored in this case, however this never happens in the tz database as it
// stands.
fn strip_comments(mut line: String) -> String {
    line.find('#').map(|pos| line.truncate(pos));
    line
}

// Generate a list of the time zone periods beyond the first that apply
// to this zone, as a string representation of a static slice.
fn format_rest(rest: Vec<(i64, FixedTimespan)>) -> String {
    let mut ret = "&[\n".to_string();
    for (start, FixedTimespan { utc_offset, dst_offset, name }) in rest {
        ret.push_str(&format!(
            "            ({start}, FixedTimespan {{ utc_offset: {utc}, dst_offset: {dst}, name: \"{name}\" }}),\n",
            start = start,
            utc = utc_offset,
            dst = dst_offset,
            name = name,
        ));
    }
    ret.push_str("        ]");
    ret
}

// Convert all '/' to '__', all '+' to 'Plus' and '-' to 'Minus', unless
// it's a hyphen, in which case remove it. This is so the names can be used
// as rust identifiers.
fn convert_bad_chars(name: &str) -> String {
    let name = name.replace("/", "__").replace("+", "Plus");
    if let Some(pos) = name.find('-') {
        if name[pos+1..].chars().next().map(char::is_numeric).unwrap_or(false) {
            name.replace("-", "Minus")
        } else {
            name.replace("-", "")
        }
    } else {
        name
    }
}

// The timezone file contains impls of `Timespans` for all timezones in the
// database. The `Wrap` wrapper in the `timezone_impl` module then implements
// TimeZone for any contained struct that implements `Timespans`.
fn write_timezone_file(timezone_file: &mut File, table: &Table) {
    let zones = table.zonesets.keys().chain(table.links.keys()).collect::<HashSet<_>>();
    write!(timezone_file, "use ::timezone_impl::{{Timespans, FixedTimespanSet, FixedTimespan}};\n\n",).unwrap();
    for zone in zones {
        let timespans = table.timespans(&zone).unwrap();
        let zone_name = convert_bad_chars(zone);
        write!(
            timezone_file,
"#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct {zone};

impl Timespans for {zone} {{
    fn this() -> Self {{ {zone} }}
    fn timespans() -> FixedTimespanSet {{
        static REST: &'static [(i64, FixedTimespan)] = {rest};
        FixedTimespanSet {{
            first: FixedTimespan {{
                utc_offset: {utc},
                dst_offset: {dst},
                name: \"{name}\",
            }},
            rest: REST
        }}
    }}
}}\n\n",
            zone = zone_name,
            rest = format_rest(timespans.rest),
            utc = timespans.first.utc_offset,
            dst = timespans.first.dst_offset,
            name = timespans.first.name,
        ).unwrap();
    }
}

// Create a file containing nice-looking re-exports such as Europe::London
// instead of having to use chrono_tz::timezones::Europe__London
fn write_directory_file(directory_file: &mut File, table: &Table) {
    for entry in table.structure() {
        if entry.name.contains('/') { continue; }
        let module_name = convert_bad_chars(entry.name);
        write!(directory_file, "pub mod {name} {{\n", name = module_name).unwrap();
        write!(directory_file, "    use timezone_impl::Wrap;\n",).unwrap();
        write!(directory_file, "    use timezones as tz;\n\n",).unwrap();
        for child in entry.children {
            match child {
                Child::Submodule(name) => {
                    let submodule_name = convert_bad_chars(name);
                    write!(directory_file, "    pub mod {name} {{\n", name = submodule_name).unwrap();
                    write!(directory_file, "        use timezone_impl::Wrap;\n",).unwrap();
                    write!(directory_file, "        use timezones as tz;\n\n",).unwrap();
                    let full_name = entry.name.to_string() + "/" + name;
                    for entry in table.structure() {
                        if entry.name == full_name {
                            for child in entry.children {
                                match child {
                                    Child::Submodule(_) => panic!("Depth of > 3 nested submodules not implemented!"),
                                    Child::TimeZone(name) => {
                                        let converted_name = convert_bad_chars(name);
                                        write!(directory_file,
                                   "        pub type {name} = Wrap<tz::{module_name}__{submodule_name}__{name}>;\n        \
                                            pub const {name} : Wrap<tz::{module_name}__{submodule_name}__{name}> = \
                                                                Wrap(tz::{module_name}__{submodule_name}__{name});\n",
                                            module_name = module_name,
                                            submodule_name = submodule_name,
                                            name = converted_name,
                                        ).unwrap();
                                    }
                                }
                            }
                        }
                    }
                    write!(directory_file, "    }}\n\n").unwrap();
                },
                Child::TimeZone(name) => {
                    let name = convert_bad_chars(name);
                    write!(directory_file,
                          "    pub type {name} = Wrap<tz::{module_name}__{name}>;\n    \
                               pub const {name} : Wrap<tz::{module_name}__{name}> = \
                                   Wrap(tz::{module_name}__{name});\n",
                          module_name = module_name,
                          name = name).unwrap();
                }
            }
        }
        write!(directory_file, "}}\n\n").unwrap();
    }
}

fn main() {
    let mut table = TableBuilder::new();
    let tzdir = Path::new("tzdb");
    let lines = std::fs::read_dir(tzdir).unwrap().into_iter()
        .map(Result::unwrap)
        .map(|direntry| direntry.path())
        .map(File::open)
        .map(Result::unwrap)
        .map(BufReader::new)
        .flat_map(BufRead::lines)
        .map(Result::unwrap)
        .map(strip_comments);
    for line in lines {
        match Line::from_str(&line).unwrap() {
            Line::Zone(zone) => table.add_zone_line(zone).unwrap(),
            Line::Continuation(cont) => table.add_continuation_line(cont).unwrap(),
            Line::Rule(rule) => table.add_rule_line(rule).unwrap(),
            Line::Link(link) => table.add_link_line(link).unwrap(),
            Line::Space => {},
        }
    }
    let table = table.build();
    let timezone_path = Path::new(&env::var("OUT_DIR").unwrap()).join("timezones.rs");
    let mut timezone_file = File::create(&timezone_path).unwrap();
    write_timezone_file(&mut timezone_file, &table);
    let directory_path = Path::new(&env::var("OUT_DIR").unwrap()).join("directory.rs");
    let mut directory_file = File::create(&directory_path).unwrap();
    write_directory_file(&mut directory_file, &table);
}
