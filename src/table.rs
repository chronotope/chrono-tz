//! Collecting parsed zoneinfo data lines into a set of time zone data.
//!
//! This module provides the `Table` struct, which is able to take parsed
//! lines of input from the `line` module and put them together, producing a
//! list of time zone transitions that can be written to files.
//!
//! It’s not as simple as it seems, because:
//!
//! 1. The zoneinfo data lines refer to each other through strings, such as
//!    “link zone A to B”; lines of that form could be *parsed* successfully
//!    but still fail to be interpreted if “B” doesn’t exist. So it has to
//!    check every step of the way. Nothing wrong with this, it’s just a
//!    consequence of reading data from a text file.
//! 2. We output the list of time zones as a set of timespans and
//!    transitions. The logic for doing this is really complicated (see
//!    ‘zic.c’, which has the same logic, only in C).

use std::collections::hash_map::{HashMap, Entry};
use std::error::Error as ErrorTrait;
use std::fmt;

use line::{self, YearSpec, MonthSpec, DaySpec, ChangeTime};
use datetime::{LocalDateTime, LocalTime};
use datetime::zone::TimeType;


/// A **table** of all the data in one or more zoneinfo files.
#[derive(PartialEq, Debug, Default)]
pub struct Table {

    /// Mapping of ruleset names to rulesets.
    pub rulesets: HashMap<String, Vec<RuleInfo>>,

    /// Mapping of zoneset names to zonesets.
    pub zonesets: HashMap<String, Vec<ZoneInfo>>,

    /// Mapping of link timezone names, to the names they link to.
    pub links: HashMap<String, String>,
}


/// A set of timespans, separated by the instances at which the timespans
/// change over. There will always be one more timespan than transitions.
///
/// This mimics the `FixedTimespanSet` struct in `datetime::cal::zone`,
/// except it uses owned `Vec`s instead of slices.
#[derive(PartialEq, Debug, Clone)]
pub struct FixedTimespanSet {

    /// The first timespan, which is assumed to have been in effect up until
    /// the initial transition instant (if any). Each set has to have at
    /// least one timespan.
    pub first: FixedTimespan,

    /// The rest of the timespans, as a vector of tuples, each containing:
    ///
    /// 1. A transition instant at which the previous timespan ends and the
    ///    next one begins, stored as a Unix timestamp;
    /// 2. The actual timespan to transition into.
    pub rest: Vec<(i64, FixedTimespan)>,
}


/// An individual timespan with a fixed offset.
///
/// This mimics the `FixedTimespan` struct in `datetime::cal::zone`, except
/// instead of “total offset” and “is DST” fields, it has separate UTC and
/// DST fields. Also, the name is an owned `String` here instead of a slice.
#[derive(PartialEq, Debug, Clone)]
pub struct FixedTimespan {

    /// The number of seconds offset from UTC during this timespan.
    pub utc_offset: i64,

    /// The number of *extra* daylight-saving seconds during this timespan.
    pub dst_offset: i64,

    /// The abbreviation in use during this timespan.
    pub name: String,
}

impl FixedTimespan {

    /// The total offset in effect during this timespan.
    pub fn total_offset(&self) -> i64 {
        self.utc_offset + self.dst_offset
    }
}


impl Table {

    /// Tries to find the zoneset with the given name by looking it up in
    /// either the zonesets map or the links map.
    pub fn get_zoneset(&self, zone_name: &str) -> &[ZoneInfo] {
        if self.zonesets.contains_key(zone_name) {
            &*self.zonesets[zone_name]
        }
        else if self.links.contains_key(zone_name) {
            let target = &self.links[zone_name];
            &*self.zonesets[&*target]
        }
        else {
            panic!("No such zone: {:?}", zone_name);
        }
    }

    /// Computes a fixed timespan set for the timezone with the given name.
    pub fn timespans(&self, zone_name: &str) -> FixedTimespanSet {
        let mut transitions = Vec::new();
        let mut start_time = None;
        let mut until_time = None;

        let mut first_transition = None;

        let zoneset = self.get_zoneset(zone_name);
        for (i, timespan) in zoneset.iter().enumerate() {
            let mut dst_offset = 0;
            let use_until      = i != zoneset.len() - 1;
            let utc_offset     = timespan.offset;

            let mut insert_start_transition = i > 0;
            let mut start_zone_id = None;
            let mut start_utc_offset = timespan.offset;
            let mut start_dst_offset = 0;

            match timespan.saving {
                Saving::NoSaving => {
                    dst_offset = 0;
                    start_zone_id = Some(timespan.format.format_constant());

                    if insert_start_transition {
                        let t = (start_time.unwrap(), FixedTimespan {
                            utc_offset: utc_offset,
                            dst_offset: dst_offset,
                            name:       start_zone_id.clone().unwrap_or("".to_owned()),
                        });
                        transitions.push(t);
                        insert_start_transition = false;
                    }
                    else {
                        first_transition = Some(FixedTimespan {
                            utc_offset: utc_offset,
                            dst_offset: dst_offset,
                            name:       start_zone_id.clone().unwrap_or("".to_owned()),
                        });
                    }
                },

                Saving::OneOff(amount) => {
                    dst_offset = amount;
                    start_zone_id = Some(timespan.format.format_constant());

                    if insert_start_transition {
                        let t = (start_time.unwrap(), FixedTimespan {
                            utc_offset: utc_offset,
                            dst_offset: dst_offset,
                            name:       start_zone_id.clone().unwrap_or("".to_owned()),
                        });
                        transitions.push(t);
                        insert_start_transition = false;
                    }
                    else {
                        first_transition = Some(FixedTimespan {
                            utc_offset: utc_offset,
                            dst_offset: dst_offset,
                            name:       start_zone_id.clone().unwrap_or("".to_owned()),
                        });
                    }
                },

                Saving::Multiple(ref rules) => {
                    use datetime::DatePiece;

                    for year in 1800..2100 {
                        if use_until && year > LocalDateTime::at(timespan.end_time.unwrap().to_timestamp()).year() {
                            break;
                        }

                        let mut activated_rules = self.rulesets[&*rules].iter()
                                                      .filter(|r| r.applies_to_year(year))
                                                      .collect::<Vec<_>>();

                        loop {
                            if use_until {
                                until_time = Some(timespan.end_time.unwrap().to_timestamp() - utc_offset - dst_offset);
                            }

                            // Find the minimum rule based on the current UTC and DST offsets.
                            // (this can be replaced with min_by when it stabilises):
                            //.min_by(|r| r.1.absolute_datetime(year, utc_offset, dst_offset));
                            let pos = {
                                let earliest = activated_rules.iter().enumerate()
                                    .map(|(i, r)| (r.absolute_datetime(year, utc_offset, dst_offset), i))
                                    .min()
                                    .map(|(_, i)| i);

                                match earliest {
                                    Some(p) => p,
                                    None    => break,
                                }
                            };

                            let earliest_rule = activated_rules.remove(pos);
                            let earliest_at = earliest_rule.absolute_datetime(year, utc_offset, dst_offset).to_instant().seconds();

                            if use_until && earliest_at >= until_time.unwrap() {
                                break;
                            }

                            dst_offset = earliest_rule.time_to_add;

                            if insert_start_transition && earliest_at == start_time.unwrap() {
                                insert_start_transition = false;
                            }

                            if insert_start_transition {
                                if earliest_at < start_time.unwrap() {
                                    start_utc_offset = timespan.offset;
                                    start_dst_offset = dst_offset;
                                    start_zone_id = Some(timespan.format.format(dst_offset, earliest_rule.letters.as_ref()));
                                    continue;
                                }

                                if start_zone_id.is_none() && start_utc_offset + start_dst_offset == timespan.offset + dst_offset {
                                    start_zone_id = Some(timespan.format.format(dst_offset, earliest_rule.letters.as_ref()));
                                }
                            }

                            let t = (earliest_at, FixedTimespan {
                                utc_offset: timespan.offset,
                                dst_offset: earliest_rule.time_to_add,
                                name:       timespan.format.format(earliest_rule.time_to_add, earliest_rule.letters.as_ref()),
                            });
                            transitions.push(t);
                        }
                    }
                }
            }

            if insert_start_transition && start_zone_id.is_some() {
                let t = (start_time.expect("Start time"), FixedTimespan {
                    utc_offset: start_utc_offset,
                    dst_offset: start_dst_offset,
                    name:       start_zone_id.clone().expect("Start zone ID"),
                });
                transitions.push(t);
            }

            if use_until {
                start_time = Some(timespan.end_time.expect("End time").to_timestamp() - utc_offset - dst_offset);
            }
        }

        transitions.sort_by(|a, b| a.0.cmp(&b.0));

        let first = match first_transition {
            Some(ft) => ft,
            None     => transitions.iter().find(|t| t.1.dst_offset == 0).unwrap().1.clone(),
        };

        let mut zoneset = FixedTimespanSet {
            first: first,
            rest:  transitions,
        };
        optimise(&mut zoneset);
        zoneset
    }
}

#[allow(unused_results)]  // for remove
fn optimise(transitions: &mut FixedTimespanSet) {
    let mut from_i = 0;
    let mut to_i = 0;

    while from_i < transitions.rest.len() {
        if to_i > 1 {
            let from = transitions.rest[from_i].0;
            let to = transitions.rest[to_i - 1].0;
            if from + transitions.rest[to_i - 1].1.total_offset() <= to + transitions.rest[to_i - 2].1.total_offset() {
                transitions.rest[to_i - 1].1 = transitions.rest[from_i].1.clone();
                from_i += 1;
                continue;
            }
        }

        if to_i == 0 || transitions.rest[to_i - 1].1 != transitions.rest[from_i].1 {
            transitions.rest[to_i] = transitions.rest[from_i].clone();
            to_i += 1;
        }

        from_i += 1
    }

    transitions.rest.truncate(to_i);

    if !transitions.rest.is_empty() && transitions.first == transitions.rest[0].1 {
        transitions.rest.remove(0);
    }
}

#[derive(PartialEq, Debug)]
pub struct RuleInfo {

    /// The year that this rule *starts* applying.
    from_year: YearSpec,

    /// The year that this rule *finishes* applying, inclusive, or `None` if
    /// it applies up until the end of this timespan.
    to_year: Option<YearSpec>,

    /// The month it applies on.
    month: MonthSpec,

    /// The day it applies on.
    day: DaySpec,

    /// The exact time it applies on.
    time: i64,

    /// The type of time that time is.
    time_type: TimeType,

    /// The amount of time to save.
    time_to_add: i64,

    /// Any extra letters that should be added to this time zone’s
    /// abbreviation, in place of `%s`.
    letters: Option<String>,
}

impl<'_> From<line::Rule<'_>> for RuleInfo {
    fn from(info: line::Rule) -> RuleInfo {
        RuleInfo {
            from_year:    info.from_year,
            to_year:      info.to_year,
            month:        info.month,
            day:          info.day,
            time:         info.time.0.as_seconds(),
            time_type:    info.time.1,
            time_to_add:  info.time_to_add.as_seconds(),
            letters:      info.letters.map(str::to_owned),
        }
    }
}

impl RuleInfo {
    fn applies_to_year(&self, year: i64) -> bool {
        use line::YearSpec::*;

        match (self.from_year, self.to_year) {
            (Number(from), None)             => year == from,
            (Number(from), Some(Maximum))    => year >= from,
            (Number(from), Some(Number(to))) => year >= from && year <= to,
            _ => unreachable!(),
        }
    }

    fn absolute_datetime(&self, year: i64, utc_offset: i64, dst_offset: i64) -> LocalDateTime {
        use datetime::Duration;

        let offset = match self.time_type {
            TimeType::UTC       => 0,
            TimeType::Standard  => utc_offset,
            TimeType::Wall      => utc_offset + dst_offset,
        };

        let date = self.day.to_concrete_date(year, self.month.0);
        let time = LocalTime::from_seconds_since_midnight(self.time);
        LocalDateTime::new(date, time) - Duration::of(offset)
    }
}

#[derive(PartialEq, Debug)]
pub struct ZoneInfo {
    pub offset:    i64,
    pub format:    Format,
    pub saving:    Saving,
    pub end_time:  Option<ChangeTime>,
}

impl<'_> From<line::ZoneInfo<'_>> for ZoneInfo {
    fn from(info: line::ZoneInfo) -> ZoneInfo {
        ZoneInfo {
            offset: info.utc_offset.as_seconds(),
            saving: match info.saving {
                line::Saving::NoSaving     => Saving::NoSaving,
                line::Saving::Multiple(s)  => Saving::Multiple(s.to_owned()),
                line::Saving::OneOff(t)    => Saving::OneOff(t.as_seconds()),
            },
            format:   Format::new(info.format),
            end_time: info.time,
        }
    }
}


/// The amount of daylight saving time (DST) to apply to this timespan. This
/// is a special type for a certain field in a zone line, which can hold
/// different types of value.
///
/// This is the owned version of the `Saving` type in the `line` module.
#[derive(PartialEq, Debug)]
pub enum Saving {

    /// Just stick to the base offset.
    NoSaving,

    /// This amount of time should be saved while this timespan is in effect.
    /// (This is the equivalent to there being a single one-off rule with the
    /// given amount of time to save).
    OneOff(i64),

    /// All rules with the given name should apply while this timespan is in
    /// effect.
    Multiple(String),
}


/// The format string to generate a time zone abbreviation from.
#[derive(PartialEq, Debug, Clone)]
pub enum Format {

    /// A constant format, which remains the same throughout both standard
    /// and DST timespans.
    Constant(String),

    /// An alternate format, such as “PST/PDT”, which changes between
    /// standard and DST timespans (the first option is standard, the second
    /// is DST).
    Alternate { standard: String, dst: String },

    /// A format with a placeholder `%s`, which uses the `letters` field in
    /// a `RuleInfo` to generate the time zone abbreviation.
    Placeholder(String),
}

impl Format {

    /// Convert the template into one of the `Format` variants. This can’t
    /// fail, as any syntax that doesn’t match one of the two formats will
    /// just be a ‘constant’ format.
    fn new(template: &str) -> Format {
        if let Some(pos) = template.find('/') {
            Format::Alternate {
                standard:  template[.. pos].to_owned(),
                dst:       template[pos + 1 ..].to_owned(),
            }
        }
        else if template.contains("%s") {
            Format::Placeholder(template.to_owned())
        }
        else {
            Format::Constant(template.to_owned())
        }
    }

    fn format(&self, dst_offset: i64, letters: Option<&String>) -> String {
        let letters = match letters {
            Some(l) => &**l,
            None    => "",
        };

        match *self {
            Format::Constant(ref s) => s.clone(),
            Format::Placeholder(ref s) => s.replace("%s", letters),
            Format::Alternate { ref standard, .. } if dst_offset == 0 => standard.clone(),
            Format::Alternate { ref dst, .. } => dst.clone(),
        }
    }

    fn format_constant(&self) -> String {
        if let Format::Constant(ref s) = *self {
            s.clone()
        }
        else {
            panic!("Expected a constant formatting string");
        }
    }
}


/// A builder for `Table` values based on various line definitions.
#[derive(PartialEq, Debug)]
pub struct TableBuilder {

    /// The table that’s being built up.
    table: Table,

    /// If the last line was a zone definition, then this holds its name.
    /// `None` otherwise. This is so continuation lines can be added to the
    /// same zone as the original zone line.
    current_zoneset_name: Option<String>,
}

impl TableBuilder {

    /// Creates a new builder with an empty table.
    pub fn new() -> TableBuilder {
        TableBuilder {
            table: Table::default(),
            current_zoneset_name: None,
        }
    }

    /// Adds a new line describing a zone definition.
    ///
    /// Returns an error if there’s already a zone with the same name, or the
    /// zone refers to a ruleset that hasn’t been defined yet.
    pub fn add_zone_line<'line>(&mut self, zone_line: line::Zone<'line>) -> Result<(), Error<'line>> {
        if let line::Saving::Multiple(ruleset_name) = zone_line.info.saving {
            if !self.table.rulesets.contains_key(ruleset_name) {
                return Err(Error::UnknownRuleset(ruleset_name));
            }
        }

        let mut zoneset = match self.table.zonesets.entry(zone_line.name.to_owned()) {
            Entry::Occupied(_)  => return Err(Error::DuplicateZone),
            Entry::Vacant(e)    => e.insert(Vec::new()),
        };

        zoneset.push(zone_line.info.into());
        self.current_zoneset_name = Some(zone_line.name.to_owned());
        Ok(())
    }

    /// Adds a new line describing the *continuation* of a zone definition.
    ///
    /// Returns an error if the builder wasn’t expecting a continuation line
    /// (meaning, the previous line wasn’t a zone line)
    pub fn add_continuation_line(&mut self, continuation_line: line::ZoneInfo) -> Result<(), Error> {
        let mut zoneset = match self.current_zoneset_name {
            Some(ref name) => self.table.zonesets.get_mut(name).unwrap(),
            None => return Err(Error::SurpriseContinuationLine),
        };

        zoneset.push(continuation_line.into());
        Ok(())
    }

    /// Adds a new line describing one entry in a ruleset, creating that set
    /// if it didn’t exist already.
    pub fn add_rule_line(&mut self, rule_line: line::Rule) -> Result<(), Error> {
        let ruleset = self.table.rulesets
                                .entry(rule_line.name.to_owned())
                                .or_insert_with(Vec::new);

        ruleset.push(rule_line.into());
        self.current_zoneset_name = None;
        Ok(())
    }

    /// Adds a new line linking one zone to another.
    ///
    /// Returns an error if there was already a link with that name.
    pub fn add_link_line<'line>(&mut self, link_line: line::Link<'line>) -> Result<(), Error<'line>> {
        match self.table.links.entry(link_line.new.to_owned()) {
            Entry::Occupied(_)  => Err(Error::DuplicateLink(link_line.new)),
            Entry::Vacant(e)    => {
                let _ = e.insert(link_line.existing.to_owned());
                self.current_zoneset_name = None;
                Ok(())
            }
        }
    }

    /// Returns the table after it’s finished being built.
    pub fn build(self) -> Table {
        self.table
    }
}


#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Error<'line> {

    /// A continuation line was passed in, but the previous line wasn’t a zone
    /// definition line.
    SurpriseContinuationLine,

    /// A zone definition referred to a ruleset that hadn’t been defined.
    UnknownRuleset(&'line str),

    /// A link line was passed in, but there’s already a link with that name.
    DuplicateLink(&'line str),

    /// A zone line was passed in, but there’s already a zone with that name.
    DuplicateZone,
}

impl<'line> fmt::Display for Error<'line> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl<'line> ErrorTrait for Error<'line> {
    fn description(&self) -> &str {
        "interpretation error"
    }

    fn cause(&self) -> Option<&ErrorTrait> {
        None
    }
}


#[cfg(test)]
mod test {

    // Allow unused results in test code, because the only ‘results’ that
    // we need to ignore are the ones from inserting and removing from
    // tables and vectors. And as we set them up ourselves, they’re bound
    // to be correct, otherwise the tests would fail!
    #![allow(unused_results)]

    use super::{FixedTimespan, FixedTimespanSet, Saving, ZoneInfo, RuleInfo, Table, Format, optimise};
    use datetime::Weekday::*;
    use datetime::Month::*;
    use datetime::zone::TimeType;
    use line::DaySpec;
    use line::WeekdaySpec;
    use line::MonthSpec;
    use line::YearSpec;
    use line::TimeSpec;
    use line::ChangeTime;

    #[test]
    fn no_transitions() {
        let zone = ZoneInfo {
            offset: 1234,
            format: Format::new("TEST"),
            saving: Saving::NoSaving,
            end_time: None,
        };

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), vec![ zone ]);

        assert_eq!(table.timespans("Test/Zone"), FixedTimespanSet {
            first: FixedTimespan { utc_offset: 1234, dst_offset: 0, name: "TEST".to_owned() },
            rest:  vec![],
        });
    }

    #[test]
    fn one_transition() {
        let zone_1 = ZoneInfo {
            offset: 1234,
            format: Format::new("TEST"),
            saving: Saving::NoSaving,
            end_time: Some(ChangeTime::UntilTime(YearSpec::Number(1970), MonthSpec(January), DaySpec::Ordinal(2), TimeSpec::HoursMinutesSeconds(10, 17, 36).with_type(TimeType::UTC))),
        };

        let zone_2 = ZoneInfo {
            offset: 5678,
            format: Format::new("TSET"),
            saving: Saving::NoSaving,
            end_time: None,
        };

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), vec![ zone_1, zone_2 ]);

        let expected = FixedTimespanSet {
            first:       FixedTimespan { utc_offset: 1234, dst_offset: 0, name: "TEST".to_owned() },
            rest: vec![
                (122222, FixedTimespan { utc_offset: 5678, dst_offset: 0, name: "TSET".to_owned() }),
            ],
        };

        assert_eq!(table.timespans("Test/Zone"), expected);
    }


    #[test]
    fn two_transitions() {
        let zone_1 = ZoneInfo {
            offset: 1234,
            format: Format::new("TEST"),
            saving: Saving::NoSaving,
            end_time: Some(ChangeTime::UntilTime(YearSpec::Number(1970), MonthSpec(January), DaySpec::Ordinal(2), TimeSpec::HoursMinutesSeconds(10, 17, 36).with_type(TimeType::Standard))),
        };

        let zone_2 = ZoneInfo {
            offset: 3456,
            format: Format::new("TSET"),
            saving: Saving::NoSaving,
            end_time: Some(ChangeTime::UntilTime(YearSpec::Number(1970), MonthSpec(January), DaySpec::Ordinal(3), TimeSpec::HoursMinutesSeconds(17, 09, 27).with_type(TimeType::Standard))),
        };

        let zone_3 = ZoneInfo {
            offset: 5678,
            format: Format::new("ESTE"),
            saving: Saving::NoSaving,
            end_time: None,
        };

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), vec![ zone_1, zone_2, zone_3 ]);

        let expected = FixedTimespanSet {
            first: FixedTimespan { utc_offset: 1234, dst_offset: 0, name: "TEST".to_owned(), },
            rest: vec![
                (122222, FixedTimespan {
                    utc_offset: 3456,
                    dst_offset: 0,
                    name: "TSET".to_owned(),
                }),
                (231111, FixedTimespan {
                    utc_offset: 5678,
                    dst_offset: 0,
                    name: "ESTE".to_owned(),
                }),
            ],
        };

        assert_eq!(table.timespans("Test/Zone"), expected);
    }

    #[test]
    fn one_rule() {
        let ruleset = vec![
            RuleInfo {
                from_year:   YearSpec::Number(1980),
                to_year:     None,
                month:       MonthSpec(February),
                day:         DaySpec::Ordinal(4),
                time:        0,
                time_type:   TimeType::UTC,
                time_to_add: 1000,
                letters:     None,
            }
        ];

        let lmt = ZoneInfo {
            offset: 0,
            format: Format::new("LMT"),
            saving: Saving::NoSaving,
            end_time: Some(ChangeTime::UntilYear(YearSpec::Number(1980))),
        };

        let zone = ZoneInfo {
            offset: 2000,
            format: Format::new("TEST"),
            saving: Saving::Multiple("Dwayne".to_owned()),
            end_time: None,
        };

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), vec![ lmt, zone ]);
        table.rulesets.insert("Dwayne".to_owned(), ruleset);

        assert_eq!(table.timespans("Test/Zone"), FixedTimespanSet {
            first: FixedTimespan { utc_offset: 0, dst_offset: 0, name: "LMT".to_owned() },
            rest:  vec![
                (318_470_400, FixedTimespan { utc_offset: 2000, dst_offset: 1000, name: "TEST".to_owned() })
            ],
        });
    }

    #[test]
    fn two_rules() {
        let ruleset = vec![
            RuleInfo {
                from_year:   YearSpec::Number(1980),
                to_year:     None,
                month:       MonthSpec(February),
                day:         DaySpec::Ordinal(4),
                time:        0,
                time_type:   TimeType::UTC,
                time_to_add: 1000,
                letters:     None,
            },
            RuleInfo {
                from_year:   YearSpec::Number(1989),
                to_year:     None,
                month:       MonthSpec(January),
                day:         DaySpec::Ordinal(12),
                time:        0,
                time_type:   TimeType::UTC,
                time_to_add: 1500,
                letters:     None,
            },
        ];

        let lmt = ZoneInfo {
            offset: 0,
            format: Format::new("LMT"),
            saving: Saving::NoSaving,
            end_time: Some(ChangeTime::UntilYear(YearSpec::Number(1980))),
        };

        let zone = ZoneInfo {
            offset: 2000,
            format: Format::new("TEST"),
            saving: Saving::Multiple("Dwayne".to_owned()),
            end_time: None,
        };

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), vec![ lmt, zone ]);
        table.rulesets.insert("Dwayne".to_owned(), ruleset);

        assert_eq!(table.timespans("Test/Zone"), FixedTimespanSet {
            first: FixedTimespan { utc_offset: 0, dst_offset: 0, name: "LMT".to_owned() },
            rest: vec![
                (318_470_400, FixedTimespan { utc_offset: 2000, dst_offset: 1000, name: "TEST".to_owned() }),
                (600_566_400, FixedTimespan { utc_offset: 2000, dst_offset: 1500, name: "TEST".to_owned() }),
            ],
        });
    }

    #[test]
    fn tripoli() {
        let libya = vec![
            RuleInfo { from_year: YearSpec::Number(1951), to_year: None,                         month: MonthSpec(October),   day: DaySpec::Ordinal(14),               time: 7200, time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1952), to_year: None,                         month: MonthSpec(January),   day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1953), to_year: None,                         month: MonthSpec(October),   day: DaySpec::Ordinal(9),                time: 7200, time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1954), to_year: None,                         month: MonthSpec(January),   day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1955), to_year: None,                         month: MonthSpec(September), day: DaySpec::Ordinal(30),               time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1956), to_year: None,                         month: MonthSpec(January),   day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1982), to_year: Some(YearSpec::Number(1984)), month: MonthSpec(April),     day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1982), to_year: Some(YearSpec::Number(1985)), month: MonthSpec(October),   day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1985), to_year: None,                         month: MonthSpec(April),     day: DaySpec::Ordinal(6),                time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1986), to_year: None,                         month: MonthSpec(April),     day: DaySpec::Ordinal(4),                time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1986), to_year: None,                         month: MonthSpec(October),   day: DaySpec::Ordinal(3),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1987), to_year: Some(YearSpec::Number(1989)), month: MonthSpec(April),     day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1987), to_year: Some(YearSpec::Number(1989)), month: MonthSpec(October),   day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1997), to_year: None,                         month: MonthSpec(April),     day: DaySpec::Ordinal(4),                time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1997), to_year: None,                         month: MonthSpec(October),   day: DaySpec::Ordinal(4),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(2013), to_year: None,                         month: MonthSpec(March),     day: DaySpec::Last(WeekdaySpec(Friday)), time: 3600, time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(2013), to_year: None,                         month: MonthSpec(October),   day: DaySpec::Last(WeekdaySpec(Friday)), time: 7200, time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
        ];

        let zone = vec![
            ZoneInfo { offset: 3164, format: Format::new("LMT"),   saving: Saving::NoSaving,                     end_time: Some(ChangeTime::UntilYear(YearSpec::Number(1920))) },
            ZoneInfo { offset: 3600, format: Format::new("CE%sT"), saving: Saving::Multiple("Libya".to_owned()), end_time: Some(ChangeTime::UntilYear(YearSpec::Number(1959)))  },
            ZoneInfo { offset: 7200, format: Format::new("EET"),   saving: Saving::NoSaving,                     end_time: Some(ChangeTime::UntilYear(YearSpec::Number(1982)))   },
            ZoneInfo { offset: 3600, format: Format::new("CE%sT"), saving: Saving::Multiple("Libya".to_owned()), end_time: Some(ChangeTime::UntilDay (YearSpec::Number(1990), MonthSpec(May),       DaySpec::Ordinal( 4)))   },
            ZoneInfo { offset: 7200, format: Format::new("EET"),   saving: Saving::NoSaving,                     end_time: Some(ChangeTime::UntilDay (YearSpec::Number(1996), MonthSpec(September), DaySpec::Ordinal(30)))   },
            ZoneInfo { offset: 3600, format: Format::new("CE%sT"), saving: Saving::Multiple("Libya".to_owned()), end_time: Some(ChangeTime::UntilDay (YearSpec::Number(1997), MonthSpec(October),   DaySpec::Ordinal( 4)))   },
            ZoneInfo { offset: 7200, format: Format::new("EET"),   saving: Saving::NoSaving,                     end_time: Some(ChangeTime::UntilTime(YearSpec::Number(2012), MonthSpec(November),  DaySpec::Ordinal(10), TimeSpec::HoursMinutes(2, 0).with_type(TimeType::Wall)))  },
            ZoneInfo { offset: 3600, format: Format::new("CE%sT"), saving: Saving::Multiple("Libya".to_owned()), end_time: Some(ChangeTime::UntilTime(YearSpec::Number(2013), MonthSpec(October),   DaySpec::Ordinal(25), TimeSpec::HoursMinutes(2, 0).with_type(TimeType::Wall)))  },
            ZoneInfo { offset: 7200, format: Format::new("EET"),   saving: Saving::NoSaving,                     end_time: None              },
        ];

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), zone);
        table.rulesets.insert("Libya".to_owned(), libya);

        assert_eq!(table.timespans("Test/Zone"), FixedTimespanSet {
            first: FixedTimespan { utc_offset: 3164,  dst_offset:    0,  name:  "LMT".to_owned() },
            rest: vec![
                (-1_577_926_364, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (  -574_902_000, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (  -568_087_200, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (  -512_175_600, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (  -504_928_800, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (  -449_888_400, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (  -441_856_800, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (  -347_158_800, FixedTimespan { utc_offset: 7200,  dst_offset:    0,  name:  "EET".to_owned() }),
                (   378_684_000, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (   386_463_600, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (   402_271_200, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (   417_999_600, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (   433_807_200, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (   449_622_000, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (   465_429_600, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (   481_590_000, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (   496_965_600, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (   512_953_200, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (   528_674_400, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (   544_230_000, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (   560_037_600, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (   575_852_400, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (   591_660_000, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (   607_388_400, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (   623_196_000, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (   641_775_600, FixedTimespan { utc_offset: 7200,  dst_offset:    0,  name:  "EET".to_owned() }),
                (   844_034_400, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                (   860_108_400, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                (   875_916_000, FixedTimespan { utc_offset: 7200,  dst_offset:    0,  name:  "EET".to_owned() }),
                ( 1_352_505_600, FixedTimespan { utc_offset: 3600,  dst_offset:    0,  name:  "CET".to_owned() }),
                ( 1_364_515_200, FixedTimespan { utc_offset: 3600,  dst_offset: 3600,  name: "CEST".to_owned() }),
                ( 1_382_659_200, FixedTimespan { utc_offset: 7200,  dst_offset:    0,  name:  "EET".to_owned() }),
            ],
        });
    }

    #[test]
    fn optimise_macquarie() {
        let mut transitions = FixedTimespanSet {
            first: FixedTimespan { utc_offset:     0, dst_offset:    0, name:  "zzz".to_owned() },
            rest: vec![
                (-2_214_259_200, FixedTimespan { utc_offset: 36000,  dst_offset:    0,  name: "AEST".to_owned() }),
                (-1_680_508_800, FixedTimespan { utc_offset: 36000,  dst_offset: 3600,  name: "AEDT".to_owned() }),
                (-1_669_892_400, FixedTimespan { utc_offset: 36000,  dst_offset: 3600,  name: "AEDT".to_owned() }),  // gets removed
                (-1_665_392_400, FixedTimespan { utc_offset: 36000,  dst_offset:    0,  name: "AEST".to_owned() }),
                (-1_601_719_200, FixedTimespan { utc_offset:     0,  dst_offset:    0,  name:  "zzz".to_owned() }),
                (  -687_052_800, FixedTimespan { utc_offset: 36000,  dst_offset:    0,  name: "AEST".to_owned() }),
                (   -94_730_400, FixedTimespan { utc_offset: 36000,  dst_offset:    0,  name: "AEST".to_owned() }),  // also gets removed
                (   -71_136_000, FixedTimespan { utc_offset: 36000,  dst_offset: 3600,  name: "AEDT".to_owned() }),
                (   -55_411_200, FixedTimespan { utc_offset: 36000,  dst_offset:    0,  name: "AEST".to_owned() }),
                (   -37_267_200, FixedTimespan { utc_offset: 36000,  dst_offset: 3600,  name: "AEDT".to_owned() }),
                (   -25_776_000, FixedTimespan { utc_offset: 36000,  dst_offset:    0,  name: "AEST".to_owned() }),
                (    -5_817_600, FixedTimespan { utc_offset: 36000,  dst_offset: 3600,  name: "AEDT".to_owned() }),
            ],
        };

        let mut result = transitions.clone();
        result.rest.remove(6);
        result.rest.remove(2);

        optimise(&mut transitions);
        assert_eq!(transitions, result);
    }

}
