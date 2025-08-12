extern crate parse_zoneinfo;
#[cfg(feature = "filter-by-regex")]
extern crate regex;

mod zoneinfo_structure;
use zoneinfo_structure::{Child, Structure};

use std::collections::BTreeSet;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use parse_zoneinfo::line::Line;
use parse_zoneinfo::table::{Table, TableBuilder};
use parse_zoneinfo::transitions::FixedTimespan;
use parse_zoneinfo::transitions::TableTransitions;
use parse_zoneinfo::FILES;

/// The name of the environment variable which possibly holds the filter regex.
#[cfg(feature = "filter-by-regex")]
pub const FILTER_ENV_VAR_NAME: &str = "CHRONO_TZ_TIMEZONE_FILTER";

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
    for (
        start,
        FixedTimespan {
            utc_offset,
            dst_offset,
            name,
        },
    ) in rest
    {
        ret.push_str(&format!(
            "                ({start}, FixedTimespan {{ \
             offset: {offset}, name: {name:?} \
             }}),\n",
            offset = utc_offset + dst_offset,
        ));
    }
    ret.push_str("            ]");
    ret
}

// Convert all '/' to '__', all '+' to 'Plus' and '-' to 'Minus', unless
// it's a hyphen, in which case remove it. This is so the names can be used
// as rust identifiers.
fn convert_bad_chars(name: &str) -> String {
    let name = name.replace('/', "__").replace('+', "Plus");
    if let Some(pos) = name.find('-') {
        if name[pos + 1..]
            .chars()
            .next()
            .map(char::is_numeric)
            .unwrap_or(false)
        {
            name.replace('-', "Minus")
        } else {
            name.replace('-', "")
        }
    } else {
        name
    }
}

// The timezone file contains impls of `Timespans` for all timezones in the
// database. The `Wrap` wrapper in the `timezone_impl` module then implements
// TimeZone for any contained struct that implements `Timespans`.
fn write_timezone_file(
    timezone_file: &mut File,
    table: &Table,
    zones: &BTreeSet<&str>,
    uncased: bool,
) -> io::Result<()> {
    writeln!(
        timezone_file,
        "use core::fmt::{{self, Debug, Display, Formatter}};",
    )?;
    writeln!(timezone_file, "use core::str::FromStr;\n",)?;
    writeln!(
        timezone_file,
        "use crate::timezone_impl::{{TimeSpans, FixedTimespanSet, FixedTimespan}};\n",
    )?;
    writeln!(
        timezone_file,
        "/// TimeZones built at compile time from the tz database
///
/// This implements [`chrono::TimeZone`] so that it may be used in and to
/// construct chrono's DateTime type. See the root module documentation
/// for details."
    )?;
    writeln!(timezone_file, "#[derive(Clone, Copy, PartialEq, Eq, Hash)]")?;
    writeln!(
        timezone_file,
        r#"#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]"#
    )?;
    writeln!(timezone_file, "pub enum Tz {{")?;
    for &zone in zones {
        let zone_name = convert_bad_chars(zone);
        writeln!(timezone_file, "    /// {zone}\n    {zone_name},")?;
    }
    writeln!(timezone_file, "}}")?;

    let mut map = phf_codegen::Map::new();
    for &zone in zones {
        map.entry(zone, format!("Tz::{}", convert_bad_chars(zone)));
    }
    writeln!(
        timezone_file,
        "static TIMEZONES: ::phf::Map<&'static str, Tz> = \n{};",
        map.build()
    )?;

    #[cfg(feature = "case-insensitive")]
    if uncased {
        writeln!(timezone_file, "use uncased::UncasedStr;\n",)?;
        let mut map = phf_codegen::Map::new();
        for &zone in zones {
            map.entry(
                uncased::UncasedStr::new(zone),
                format!("Tz::{}", convert_bad_chars(zone)),
            );
        }
        writeln!(
            timezone_file,
            "static TIMEZONES_UNCASED: ::phf::Map<&'static uncased::UncasedStr, Tz> = \n{};",
            map.build()
        )?;
    }

    writeln!(
        timezone_file,
        r#"#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ParseError(());

impl Display for ParseError {{
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {{
        f.write_str("failed to parse timezone")
    }}
}}

#[cfg(feature = "std")]
impl std::error::Error for ParseError {{}}

impl FromStr for Tz {{
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {{
        TIMEZONES.get(s).cloned().ok_or(ParseError(()))
    }}
}}
"#
    )?;

    writeln!(
        timezone_file,
        "impl Tz {{
    pub fn name(self) -> &'static str {{
        match self {{"
    )?;
    for &zone in zones {
        let zone_name = convert_bad_chars(zone);
        writeln!(timezone_file, "            Tz::{zone_name} => \"{zone}\",")?;
    }
    writeln!(
        timezone_file,
        "        }}
    }}"
    )?;

    if uncased {
        writeln!(
            timezone_file,
            r#"
    #[cfg(feature = "case-insensitive")]
    /// Parses a timezone string in a case-insensitive way
    pub fn from_str_insensitive(s: &str) -> Result<Self, ParseError> {{
        return TIMEZONES_UNCASED.get(s.into()).cloned().ok_or(ParseError(()));
    }}"#
        )?;
    }

    writeln!(timezone_file, "}}")?;

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
    fn timespans(&self) -> FixedTimespanSet {{"
    )?;
    for zone in zones
        .iter()
        .map(|&z| table.links.get(z).map(String::as_str).unwrap_or(z))
        .collect::<BTreeSet<_>>()
    {
        let zone_name = convert_bad_chars(zone);
        let timespans = table.timespans(zone).unwrap();
        writeln!(
            timezone_file,
            "        const {zone}: FixedTimespanSet = FixedTimespanSet {{
            first: FixedTimespan {{ offset: {offset}, name: {name:?} }},
            rest: {rest},
        }};\n",
            zone = zone_name.to_uppercase(),
            rest = format_rest(timespans.rest),
            offset = timespans.first.utc_offset + timespans.first.dst_offset,
            name = timespans.first.name,
        )?;
    }

    write!(
        timezone_file,
        "
        match *self {{
"
    )?;

    for &zone in zones {
        let zone_name = convert_bad_chars(zone);
        let target_name = if let Some(target) = table.links.get(zone) {
            convert_bad_chars(target)
        } else {
            zone_name.clone()
        };
        writeln!(
            timezone_file,
            "            Tz::{zone_name} => {target_name},",
            target_name = target_name.to_uppercase(),
        )?;
    }
    write!(
        timezone_file,
        "        }}
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
    for &zone in zones {
        writeln!(
            timezone_file,
            "    Tz::{zone},",
            zone = convert_bad_chars(zone)
        )?;
    }
    write!(timezone_file, "];")?;
    Ok(())
}

// Create a file containing nice-looking re-exports such as Europe::London
// instead of having to use chrono_tz::timezones::Europe__London
fn write_directory_file(
    directory_file: &mut File,
    table: &Table,
    zones: &BTreeSet<&str>,
    version: &str,
) -> io::Result<()> {
    writeln!(directory_file, "use crate::timezones::Tz;\n")?;

    // expose the underlying IANA TZDB version
    writeln!(
        directory_file,
        "pub const IANA_TZDB_VERSION: &str = \"{version}\";\n"
    )?;

    // add the `loose' zone definitions first
    for &zone in zones.iter().filter(|zone| !zone.contains('/')) {
        let zone = convert_bad_chars(zone);
        writeln!(directory_file, "pub const {zone}: Tz = Tz::{zone};")?;
    }
    writeln!(directory_file)?;

    // now add the `structured' zone names in submodules
    let mut first = true;
    for entry in zoneinfo_structure::build_tree(zones.iter().copied()) {
        if entry.name.contains('/') {
            continue;
        }

        match first {
            true => first = false,
            false => writeln!(directory_file, "")?,
        }

        let module_name = convert_bad_chars(entry.name);
        writeln!(directory_file, "pub mod {module_name} {{")?;
        writeln!(directory_file, "    use super::*;\n",)?;
        for child in entry.children {
            let name = match child {
                Child::Submodule(name) => name,
                Child::TimeZone(name) => {
                    let name = convert_bad_chars(name);
                    writeln!(
                        directory_file,
                        "    pub const {name}: Tz = Tz::{module_name}__{name};"
                    )?;
                    continue;
                }
            };

            let submodule_name = convert_bad_chars(name);
            writeln!(directory_file, "    pub mod {submodule_name} {{")?;
            writeln!(directory_file, "        use crate::timezones::Tz;\n",)?;
            let full_name = entry.name.to_string() + "/" + name;
            for entry in table.structure() {
                if entry.name != full_name {
                    continue;
                }

                for child in entry.children {
                    let name = match child {
                        Child::Submodule(_) => {
                            panic!("Depth of > 3 nested submodules not implemented!")
                        }
                        Child::TimeZone(name) => name,
                    };

                    let converted_name = convert_bad_chars(name);
                    writeln!(directory_file,
                        "        pub const {converted_name}: Tz = Tz::{module_name}__{submodule_name}__{converted_name};",
                    )?;
                }
            }
            writeln!(directory_file, "    }}\n")?;
        }
        writeln!(directory_file, "}}")?;
    }

    Ok(())
}

/// Checks the `CHRONO_TZ_TIMEZONE_FILTER` environment variable.
/// Converts it to a regex if set. Panics if the regex is not valid, as we want
/// to fail the build if that happens.
#[cfg(feature = "filter-by-regex")]
fn get_filter_regex() -> Option<regex::Regex> {
    match std::env::var(FILTER_ENV_VAR_NAME) {
        Ok(val) => {
            let val = val.trim();
            if val.is_empty() {
                return None;
            }
            match regex::Regex::new(val) {
                    Ok(regex) => Some(regex),
                    Err(err) => panic!(
                        "The value '{val:?}' for environment variable {FILTER_ENV_VAR_NAME} is not a valid regex, err={err}"
                    ),
                }
        }
        Err(env::VarError::NotPresent) => None,
        Err(env::VarError::NotUnicode(s)) => panic!(
            "The value '{s:?}' for environment variable {FILTER_ENV_VAR_NAME} is not valid Unicode"
        ),
    }
}

fn detect_iana_db_version() -> String {
    let root = env::var("CARGO_MANIFEST_DIR").expect("no Cargo build context");
    let path = Path::new(&root).join(Path::new("tz/NEWS"));
    let file = File::open(path).expect("failed to open file");

    let mut lines = BufReader::new(file).lines();
    while let Some(Ok(line)) = lines.next() {
        let line = match line.strip_prefix("Release ") {
            Some(line) => line,
            _ => continue,
        };

        match line.split_once(" - ") {
            Some((version, _)) => return version.to_owned(),
            _ => continue,
        }
    }

    unreachable!("no version found")
}

pub fn main(dir: &Path, _filter: bool, uncased: bool) {
    let mut table = TableBuilder::new();

    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| String::new()));
    for fname in FILES {
        let path = root.join(format!("tz/{fname}"));
        let file =
            File::open(&path).unwrap_or_else(|e| panic!("cannot open {}: {e}", path.display()));
        for line in BufReader::new(file).lines() {
            let line = strip_comments(line.unwrap());
            table.add_line(Line::new(&line).unwrap()).unwrap();
        }
    }

    let table = table.build();

    #[cfg(feature = "filter-by-regex")]
    let regex = _filter.then(get_filter_regex).flatten();
    #[cfg(feature = "filter-by-regex")]
    let filter = |tz: &str| regex.as_ref().is_none_or(|r| r.is_match(tz));
    #[cfg(not(feature = "filter-by-regex"))]
    let filter = |_: &str| true;

    let zones = table
        .zonesets
        .keys()
        .chain(table.links.keys())
        .filter(|s| filter(s))
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    let timezone_path = dir.join("timezones.rs");
    let mut timezone_file = File::create(timezone_path).unwrap();
    write_timezone_file(&mut timezone_file, &table, &zones, uncased).unwrap();

    let directory_path = dir.join("directory.rs");
    let mut directory_file = File::create(directory_path).unwrap();
    let version = detect_iana_db_version();
    write_directory_file(&mut directory_file, &table, &zones, &version).unwrap();
}
