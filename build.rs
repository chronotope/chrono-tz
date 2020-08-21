extern crate parse_zoneinfo;

use std::collections::BTreeSet;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use parse_zoneinfo::line::{Line, LineParser};
use parse_zoneinfo::structure::{Child, Structure};
use parse_zoneinfo::table::{Table, TableBuilder};
use parse_zoneinfo::transitions::FixedTimespan;
use parse_zoneinfo::transitions::TableTransitions;

// This function is needed until zoneinfo_parse handles comments correctly.
// Technically a '#' symbol could occur between double quotes and should be
// ignored in this case, however this never happens in the tz database as it
// stands.
fn strip_comments(mut line: String) -> String {
    if let Some(pos) = line.find('#') {
        line.truncate(pos);
    };
    line
}

// Generate a list of the time zone periods beyond the first that apply
// to this zone, as a string representation of a static slice.
fn format_rest(rest: Vec<(i64, FixedTimespan)>) -> String {
    let mut ret = "&[\n".to_string();
    for (start, FixedTimespan { utc_offset, dst_offset, name }) in rest {
        ret.push_str(&format!(
            "                    ({start}, FixedTimespan {{ \
             utc_offset: {utc}, dst_offset: {dst}, name: \"{name}\" \
             }}),\n",
            start = start,
            utc = utc_offset,
            dst = dst_offset,
            name = name,
        ));
    }
    ret.push_str("                ]");
    ret
}

// Convert all '/' to '__', all '+' to 'Plus' and '-' to 'Minus', unless
// it's a hyphen, in which case remove it. This is so the names can be used
// as rust identifiers.
fn convert_bad_chars(name: &str) -> String {
    let name = name.replace("/", "__").replace("+", "Plus");
    if let Some(pos) = name.find('-') {
        if name[pos + 1..].chars().next().map(char::is_numeric).unwrap_or(false) {
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
fn write_timezone_file(timezone_file: &mut File, table: &Table) -> io::Result<()> {
    let zones = table.zonesets.keys().chain(table.links.keys()).collect::<BTreeSet<_>>();
    writeln!(timezone_file, "use core::fmt::{{self, Debug, Display, Formatter}};",)?;
    writeln!(timezone_file, "use core::str::FromStr;\n",)?;
    writeln!(
        timezone_file,
        "use ::timezone_impl::{{TimeSpans, FixedTimespanSet, FixedTimespan}};\n",
    )?;
    writeln!(
        timezone_file,
        "/// TimeZones built at compile time from the tz database
///
/// This implements [`chrono::TimeZone`] so that it may be used in and to
/// construct chrono's DateTime type. See the root module documentation
/// for details."
    )?;
    writeln!(timezone_file, "#[derive(Clone, Copy, PartialEq, Eq, Hash)]\npub enum Tz {{")?;
    for zone in &zones {
        let zone_name = convert_bad_chars(zone);
        writeln!(
            timezone_file,
            "    /// {raw_zone_name}\n    {zone},",
            zone = zone_name,
            raw_zone_name = zone
        )?;
    }
    writeln!(timezone_file, "}}")?;

    writeln!(
        timezone_file,
        "impl FromStr for Tz {{
    #[cfg(feature = \"std\")]
    type Err = String;
    #[cfg(not(feature = \"std\"))]
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {{
        match s {{"
    )?;
    for zone in &zones {
        let zone_name = convert_bad_chars(zone);
        writeln!(
            timezone_file,
            "            \"{raw_zone_name}\" => Ok(Tz::{zone}),",
            zone = zone_name,
            raw_zone_name = zone
        )?;
    }
    writeln!(
        timezone_file,
        "            #[cfg(feature = \"std\")]
            s => Err(format!(\"'{{}}' is not a valid timezone\", s.to_string())),
            #[cfg(not(feature = \"std\"))]
            _ => Err(\"received invalid timezone\"),
        }}
    }}
}}\n"
    )?;

    writeln!(
        timezone_file,
        "impl Tz {{
    pub fn name(self) -> &'static str {{
        match self {{"
    )?;
    for zone in &zones {
        let zone_name = convert_bad_chars(zone);
        writeln!(
            timezone_file,
            "            Tz::{zone} => \"{raw_zone_name}\",",
            zone = zone_name,
            raw_zone_name = zone
        )?;
    }
    writeln!(
        timezone_file,
        "        }}
    }}
}}\n"
    )?;
    writeln!(
        timezone_file,
        "impl Debug for Tz {{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {{
        f.write_str(self.name().as_ref())
    }}
}}\n"
    )?;
    writeln!(
        timezone_file,
        "impl Display for Tz {{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {{
        f.write_str(self.name().as_ref())
    }}
}}\n"
    )?;
    writeln!(
        timezone_file,
        "impl TimeSpans for Tz {{
    fn timespans(&self) -> FixedTimespanSet {{
        match *self {{"
    )?;
    for zone in &zones {
        let timespans = table.timespans(&zone).unwrap();
        let zone_name = convert_bad_chars(zone);
        writeln!(
            timezone_file,
            "            Tz::{zone} => {{
                const REST: &[(i64, FixedTimespan)] = {rest};
                FixedTimespanSet {{
                    first: FixedTimespan {{
                        utc_offset: {utc},
                        dst_offset: {dst},
                        name: \"{name}\",
                    }},
                    rest: REST
                }}
            }},\n",
            zone = zone_name,
            rest = format_rest(timespans.rest),
            utc = timespans.first.utc_offset,
            dst = timespans.first.dst_offset,
            name = timespans.first.name,
        )?;
    }
    write!(
        timezone_file,
        "         }}
    }}
}}\n"
    )?;
    write!(
        timezone_file,
        "/// An array of every known variant
///
/// Useful for iterating over known timezones:
///
/// ```
/// use chrono_tz::{{TZ_VARIANTS, Tz}};
/// assert!(TZ_VARIANTS.iter().any(|v| *v == Tz::UTC));
/// ```
pub static TZ_VARIANTS: [Tz; {num}] = [
",
        num = zones.len()
    )?;
    for zone in &zones {
        writeln!(timezone_file, "    Tz::{zone},", zone = convert_bad_chars(zone))?;
    }
    write!(timezone_file, "];")?;
    Ok(())
}

// Create a file containing nice-looking re-exports such as Europe::London
// instead of having to use chrono_tz::timezones::Europe__London
fn write_directory_file(directory_file: &mut File, table: &Table) -> io::Result<()> {
    // add the `loose' zone definitions first at the top of the file
    writeln!(directory_file, "use timezones::Tz;\n")?;
    let zones = table
        .zonesets
        .keys()
        .chain(table.links.keys())
        .filter(|zone| !zone.contains('/'))
        .collect::<BTreeSet<_>>();
    for zone in zones {
        let zone = convert_bad_chars(zone);
        writeln!(directory_file, "pub const {name} : Tz = Tz::{name};", name = zone)?;
    }
    writeln!(directory_file)?;

    // now add the `structured' zone names in submodules
    for entry in table.structure() {
        if entry.name.contains('/') {
            continue;
        }
        let module_name = convert_bad_chars(entry.name);
        writeln!(directory_file, "pub mod {name} {{", name = module_name)?;
        writeln!(directory_file, "    use timezones::Tz;\n",)?;
        for child in entry.children {
            match child {
                Child::Submodule(name) => {
                    let submodule_name = convert_bad_chars(name);
                    writeln!(directory_file, "    pub mod {name} {{", name = submodule_name)?;
                    writeln!(directory_file, "        use timezones::Tz;\n",)?;
                    let full_name = entry.name.to_string() + "/" + name;
                    for entry in table.structure() {
                        if entry.name == full_name {
                            for child in entry.children {
                                match child {
                                    Child::Submodule(_) => {
                                        panic!("Depth of > 3 nested submodules not implemented!")
                                    }
                                    Child::TimeZone(name) => {
                                        let converted_name = convert_bad_chars(name);
                                        writeln!(directory_file,
                                    "        pub const {name} : Tz = Tz::{module_name}__{submodule_name}__{name};",
                                            module_name = module_name,
                                            submodule_name = submodule_name,
                                            name = converted_name,
                                        )?;
                                    }
                                }
                            }
                        }
                    }
                    writeln!(directory_file, "    }}\n")?;
                }
                Child::TimeZone(name) => {
                    let name = convert_bad_chars(name);
                    writeln!(
                        directory_file,
                        "    pub const {name} : Tz = Tz::{module_name}__{name};",
                        module_name = module_name,
                        name = name
                    )?;
                }
            }
        }
        writeln!(directory_file, "}}\n")?;
    }
    Ok(())
}

fn main() {
    let parser = LineParser::new();
    let mut table = TableBuilder::new();

    let tzfiles = [
        "tz/africa",
        "tz/antarctica",
        "tz/asia",
        "tz/australasia",
        "tz/backward",
        "tz/etcetera",
        "tz/europe",
        "tz/northamerica",
        "tz/pacificnew",
        "tz/southamerica",
    ];

    let lines = tzfiles
        .iter()
        .map(Path::new)
        .map(|p| Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap_or("".to_string())).join(p))
        .map(File::open)
        .map(Result::unwrap)
        .map(BufReader::new)
        .flat_map(BufRead::lines)
        .map(Result::unwrap)
        .map(strip_comments);

    for line in lines {
        match parser.parse_str(&line).unwrap() {
            Line::Zone(zone) => table.add_zone_line(zone).unwrap(),
            Line::Continuation(cont) => table.add_continuation_line(cont).unwrap(),
            Line::Rule(rule) => table.add_rule_line(rule).unwrap(),
            Line::Link(link) => table.add_link_line(link).unwrap(),
            Line::Space => {}
        }
    }
    let table = table.build();
    let timezone_path = Path::new(&env::var("OUT_DIR").unwrap()).join("timezones.rs");
    let mut timezone_file = File::create(&timezone_path).unwrap();
    write_timezone_file(&mut timezone_file, &table).unwrap();
    let directory_path = Path::new(&env::var("OUT_DIR").unwrap()).join("directory.rs");
    let mut directory_file = File::create(&directory_path).unwrap();
    write_directory_file(&mut directory_file, &table).unwrap();
}
