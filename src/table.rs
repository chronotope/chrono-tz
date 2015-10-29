use std::collections::hash_map::{HashMap, Entry};

use line::{self, YearSpec, MonthSpec, DaySpec, ZoneTime};
use datetime::local::{LocalDateTime, LocalTime};
use datetime::zoned::zoneinfo::TimeType;


/// A table of all the data in one or more zoneinfo files.
#[derive(PartialEq, Debug, Default)]
pub struct Table {

    /// Mapping of ruleset names to rulesets.
    pub rulesets: HashMap<String, Ruleset>,

    /// Mapping of zoneset names to zonesets.
    pub zonesets: HashMap<String, Zoneset>,

    /// Mapping of link timezone names, to the names they link to.
    pub links: HashMap<String, String>,
}

#[derive(PartialEq, Debug, Default)]
pub struct Ruleset(pub Vec<RuleInfo>);

#[derive(PartialEq, Debug, Clone)]
pub struct Transition {
    pub occurs_at:  Option<i64>,
    pub utc_offset: i64,
    pub dst_offset: i64,
    pub name:       String,
}

impl Transition {
    pub fn total_offset(&self) -> i64 {
        self.utc_offset + self.dst_offset
    }
}

impl Table {
    pub fn transitions(&self, zone_name: &str) -> Vec<Transition> {
        let mut transitions = Vec::new();
        let mut start_time = None;
        let mut until_time = None;

        let timespans = &self.zonesets[zone_name];
        for (i, timespan) in timespans.0.iter().enumerate() {
            let mut dst_offset = 0;
            let use_until      = i != timespans.0.len() - 1;
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
                        let t = Transition {
                            occurs_at:  Some(start_time.unwrap()),
                            utc_offset: utc_offset,
                            dst_offset: dst_offset,
                            name:       start_zone_id.clone().unwrap_or("".to_string()),
                        };
                        transitions.push(t);
                        insert_start_transition = false;
                    }
                    else {
                        let t = Transition {
                            occurs_at:  None,
                            utc_offset: utc_offset,
                            dst_offset: dst_offset,
                            name:       start_zone_id.clone().unwrap_or("".to_string()),
                        };
                        transitions.push(t);
                    }
                },

                Saving::OneOff(amount) => {
                    dst_offset = amount;
                    start_zone_id = Some(timespan.format.format_constant());

                    if insert_start_transition {
                        let t = Transition {
                            occurs_at:  Some(start_time.unwrap()),
                            utc_offset: utc_offset,
                            dst_offset: dst_offset,
                            name:       start_zone_id.clone().unwrap_or("".to_string()),
                        };
                        transitions.push(t);
                        insert_start_transition = false;
                    }
                    else {
                        let t = Transition {
                            occurs_at:  None,
                            utc_offset: utc_offset,
                            dst_offset: dst_offset,
                            name:       start_zone_id.clone().unwrap_or("".to_string()),
                        };
                        transitions.push(t);
                    }
                },

                Saving::Multiple(ref rules) => {
                    use datetime::local::DatePiece;

                    for year in 1800..2100 {
                        if use_until && year > LocalDateTime::at(timespan.end_time.unwrap().to_timestamp()).year() {
                            break;
                        }

                        let mut activated_rules = self.rulesets[&*rules].0.iter()
                                                      .filter(|r| r.applies_to_year(year))
                                                      .collect::<Vec<_>>();

                        loop {
                            if use_until {
                                until_time = Some(timespan.end_time.unwrap().to_timestamp() - utc_offset - dst_offset);
                            }

                            let pos = {
                                let earliest = activated_rules.iter().enumerate()
                                    .min_by(|r| r.1.absolute_datetime(year, utc_offset, dst_offset));

                                match earliest {
                                    Some((p, _)) => p,
                                    None         => break,
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

                            let t = Transition {
                                occurs_at:  Some(earliest_at),
                                utc_offset: timespan.offset,
                                dst_offset: earliest_rule.time_to_add,
                                name:       timespan.format.format(earliest_rule.time_to_add, earliest_rule.letters.as_ref()),
                            };
                            transitions.push(t);
                        }
                    }
                }
            }

            if insert_start_transition {
                let t = Transition {
                    occurs_at:  Some(start_time.unwrap()),
                    utc_offset: start_utc_offset,
                    dst_offset: start_dst_offset,
                    name:       start_zone_id.clone().unwrap(),
                };
                transitions.push(t);
            }

            if use_until {
                start_time = Some(timespan.end_time.unwrap().to_timestamp() - utc_offset - dst_offset);
            }
        }

        transitions.sort_by(|a, b| a.occurs_at.cmp(&b.occurs_at));
        optimise(&mut transitions);
        transitions
    }
}

fn optimise(transitions: &mut Vec<Transition>) {
    let mut from_i = 0;
    let mut to_i = 0;

    while from_i < transitions.len() {
        if to_i > 1 {
            if let (Some(from), Some(to)) = (transitions[from_i].occurs_at, transitions[to_i - 1].occurs_at) {
                if from + transitions[to_i - 1].total_offset() <= to + transitions[to_i - 2].total_offset() {
                    let replacement_transition = Transition {
                        occurs_at:  transitions[to_i - 1].occurs_at,
                        name:       transitions[from_i].name.clone(),
                        utc_offset: transitions[from_i].utc_offset,
                        dst_offset: transitions[from_i].dst_offset,
                    };

                    transitions[to_i - 1] = replacement_transition;
                    from_i += 1;
                    continue;
                }
            }

        }

        if to_i == 0
        || transitions[to_i - 1].utc_offset != transitions[from_i].utc_offset
        || transitions[to_i - 1].dst_offset != transitions[from_i].dst_offset
        || transitions[to_i - 1].name       != transitions[from_i].name {
            transitions[to_i] = transitions[from_i].clone();
            to_i += 1;
        }

        from_i += 1
    }

    if to_i > 0 {
        transitions.truncate(to_i);
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

    /// Any extra letters that should be added to this time zone's
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
        use datetime::duration::Duration;

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

#[derive(PartialEq, Debug, Default)]
pub struct Zoneset(pub Vec<ZoneInfo>);

#[derive(PartialEq, Debug)]
pub struct ZoneInfo {
    pub offset:    i64,
    pub format:    Format,
    pub saving:    Saving,
    pub end_time:  Option<ZoneTime>,
}

#[derive(PartialEq, Debug)]
pub enum Saving {
    NoSaving,
    OneOff(i64),
    Multiple(String),
}

#[derive(PartialEq, Debug, Clone)]
pub enum Format {
    Constant(String),
    Alternate { standard: String, dst: String },
    Placeholder(String),
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

impl Format {
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

    /// The table that's being built up.
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
    /// Returns an error if there's already a zone with the same name, or the
    /// zone refers to a ruleset that hasn't been defined yet.
    pub fn add_zone_line<'line>(&mut self, zone_line: line::Zone<'line>) -> Result<(), Error<'line>> {
        if let line::Saving::Multiple(ruleset_name) = zone_line.info.saving {
            if !self.table.rulesets.contains_key(ruleset_name) {
                return Err(Error::UnknownRuleset(ruleset_name));
            }
        }

        let mut zoneset = match self.table.zonesets.entry(zone_line.name.to_owned()) {
            Entry::Occupied(_)  => return Err(Error::DuplicateZone),
            Entry::Vacant(e)    => e.insert(Zoneset(Vec::new())),
        };

        let _ = zoneset.0.push(zone_line.info.into());
        self.current_zoneset_name = Some(zone_line.name.to_owned());
        Ok(())
    }

    /// Adds a new line describing the *continuation* of a zone definition.
    ///
    /// Returns an error if the builder wasn't expecting a continuation line
    /// (meaning, the previous line wasn't a zone line)
    pub fn add_continuation_line(&mut self, continuation_line: line::ZoneInfo) -> Result<(), Error> {
        let mut zoneset = match self.current_zoneset_name {
            Some(ref name) => self.table.zonesets.get_mut(name).unwrap(),
            None => return Err(Error::SurpriseContinuationLine),
        };

        let _ = zoneset.0.push(continuation_line.into());
        Ok(())
    }

    /// Adds a new line describing one entry in a ruleset, creating that set
    /// if it didn't exist already.
    pub fn add_rule_line(&mut self, rule_line: line::Rule) -> Result<(), Error> {
        let ruleset = self.table.rulesets
                                .entry(rule_line.name.to_owned())
                                .or_insert_with(|| Ruleset(Vec::new()));

        ruleset.0.push(rule_line.into());
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

    /// Returns the table after it's finished being built.
    pub fn build(self) -> Table {
        self.table
    }
}


#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Error<'line> {

    /// A continuation line was passed in, but the previous line wasn't a zone
    /// definition line.
    SurpriseContinuationLine,

    /// A zone definition referred to a ruleset that hadn't been defined.
    UnknownRuleset(&'line str),

    /// A link line was passed in, but there's already a link with that name.
    DuplicateLink(&'line str),

    /// A zone line was passed in, but there's already a zone with that name.
    DuplicateZone,
}


#[cfg(test)]
mod test {
    use super::{Transition, Saving, ZoneInfo, RuleInfo, Ruleset, Table, Zoneset, Format, optimise};
    use datetime::local::Weekday::*;
    use datetime::local::Month::*;
    use datetime::zoned::zoneinfo::TimeType;
    use line::DaySpec;
    use line::WeekdaySpec;
    use line::MonthSpec;
    use line::YearSpec;
    use line::TimeSpec;
    use line::ZoneTime;

    #[test]
    fn no_transitions() {
        let zone = ZoneInfo {
            offset: 1234,
            format: Format::new("TEST"),
            saving: Saving::NoSaving,
            end_time: None,
        };

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), Zoneset(vec![ zone ]));

        assert_eq!(table.transitions("Test/Zone"), vec![
            Transition {
                occurs_at:  None,
                utc_offset: 1234,
                dst_offset: 0,
                name:       "TEST".to_owned(),
            }
        ]);
    }

    #[test]
    fn one_transition() {
        let zone_1 = ZoneInfo {
            offset: 1234,
            format: Format::new("TEST"),
            saving: Saving::NoSaving,
            end_time: Some(ZoneTime::UntilTime(YearSpec::Number(1970), MonthSpec(January), DaySpec::Ordinal(2), TimeSpec::HoursMinutesSeconds(10, 17, 36).with_type(TimeType::UTC))),
        };

        let zone_2 = ZoneInfo {
            offset: 5678,
            format: Format::new("TSET"),
            saving: Saving::NoSaving,
            end_time: None,
        };

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), Zoneset(vec![ zone_1, zone_2 ]));

        assert_eq!(table.transitions("Test/Zone"), vec![
            Transition {
                occurs_at: None,
                utc_offset: 1234,
                dst_offset: 0,
                name:       "TEST".to_owned(),
            },
            Transition {
                occurs_at: Some(122222),
                utc_offset: 5678,
                dst_offset: 0,
                name:       "TSET".to_owned(),
            },
        ]);
    }


    #[test]
    fn two_transitions() {
        let zone_1 = ZoneInfo {
            offset: 1234,
            format: Format::new("TEST"),
            saving: Saving::NoSaving,
            end_time: Some(ZoneTime::UntilTime(YearSpec::Number(1970), MonthSpec(January), DaySpec::Ordinal(2), TimeSpec::HoursMinutesSeconds(10, 17, 36).with_type(TimeType::Standard))),
        };

        let zone_2 = ZoneInfo {
            offset: 3456,
            format: Format::new("TSET"),
            saving: Saving::NoSaving,
            end_time: Some(ZoneTime::UntilTime(YearSpec::Number(1970), MonthSpec(January), DaySpec::Ordinal(3), TimeSpec::HoursMinutesSeconds(17, 09, 27).with_type(TimeType::Standard))),
        };

        let zone_3 = ZoneInfo {
            offset: 5678,
            format: Format::new("ESTE"),
            saving: Saving::NoSaving,
            end_time: None,
        };

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), Zoneset(vec![ zone_1, zone_2, zone_3 ]));

        assert_eq!(table.transitions("Test/Zone"), vec![
            Transition {
                occurs_at: None,
                utc_offset: 1234,
                dst_offset: 0,
                name: "TEST".to_owned(),
            },
            Transition {
                occurs_at: Some(122222),
                utc_offset: 3456,
                dst_offset: 0,
                name: "TSET".to_owned(),
            },
            Transition {
                occurs_at: Some(231111),
                utc_offset: 5678,
                dst_offset: 0,
                name: "ESTE".to_owned(),
            },
        ]);
    }

    #[test]
    fn one_rule() {
        let ruleset = Ruleset(vec![
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
        ]);

        let zone = ZoneInfo {
            offset: 2000,
            format: Format::new("TEST"),
            saving: Saving::Multiple("Dwayne".to_owned()),
            end_time: None,
        };

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), Zoneset(vec![ zone ]));
        table.rulesets.insert("Dwayne".to_owned(), ruleset);

        assert_eq!(table.transitions("Test/Zone"), vec![
            Transition {
                occurs_at:  Some(318_470_400),
                utc_offset: 2000,
                dst_offset: 1000,
                name:       "TEST".to_owned(),
            },
        ]);
    }

    #[test]
    fn two_rules() {
        let ruleset = Ruleset(vec![
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
        ]);

        let zone = ZoneInfo {
            offset: 2000,
            format: Format::new("TEST"),
            saving: Saving::Multiple("Dwayne".to_owned()),
            end_time: None,
        };

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), Zoneset(vec![ zone ]));
        table.rulesets.insert("Dwayne".to_owned(), ruleset);

        assert_eq!(table.transitions("Test/Zone"), vec![
            Transition {
                occurs_at:  Some(318_470_400),
                utc_offset: 2000,
                dst_offset: 1000,
                name:       "TEST".to_owned(),
            },
            Transition {
                occurs_at:  Some(600_566_400),
                utc_offset: 2000,
                dst_offset: 1500,
                name:       "TEST".to_owned(),
            },
        ]);
    }

    #[test]
    fn tripoli() {
        let libya = Ruleset(vec![
            RuleInfo { from_year: YearSpec::Number(1951), to_year: None,               month: MonthSpec(October),   day: DaySpec::Ordinal(14),               time: 7200, time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1952), to_year: None,               month: MonthSpec(January),   day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1953), to_year: None,               month: MonthSpec(October),   day: DaySpec::Ordinal(9),                time: 7200, time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1954), to_year: None,               month: MonthSpec(January),   day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1955), to_year: None,               month: MonthSpec(September), day: DaySpec::Ordinal(30),               time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1956), to_year: None,               month: MonthSpec(January),   day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1982), to_year: Some(YearSpec::Number(1984)), month: MonthSpec(April),     day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1982), to_year: Some(YearSpec::Number(1985)), month: MonthSpec(October),   day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1985), to_year: None,               month: MonthSpec(April),     day: DaySpec::Ordinal(6),                time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1986), to_year: None,               month: MonthSpec(April),     day: DaySpec::Ordinal(4),                time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1986), to_year: None,               month: MonthSpec(October),   day: DaySpec::Ordinal(3),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1987), to_year: Some(YearSpec::Number(1989)), month: MonthSpec(April),     day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1987), to_year: Some(YearSpec::Number(1989)), month: MonthSpec(October),   day: DaySpec::Ordinal(1),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(1997), to_year: None,               month: MonthSpec(April),     day: DaySpec::Ordinal(4),                time: 0,    time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(1997), to_year: None,               month: MonthSpec(October),   day: DaySpec::Ordinal(4),                time: 0,    time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
            RuleInfo { from_year: YearSpec::Number(2013), to_year: None,               month: MonthSpec(March),     day: DaySpec::Last(WeekdaySpec(Friday)), time: 3600, time_type: TimeType::Wall, time_to_add: 3600, letters: Some("S".to_owned()) },
            RuleInfo { from_year: YearSpec::Number(2013), to_year: None,               month: MonthSpec(October),   day: DaySpec::Last(WeekdaySpec(Friday)), time: 7200, time_type: TimeType::Wall, time_to_add: 0,    letters: None                 },
        ]);

        let zone = vec![
            ZoneInfo { offset: 3164, format: Format::new("LMT"),   saving: Saving::NoSaving,                     end_time: Some(ZoneTime::UntilYear(YearSpec::Number(1920))) },
            ZoneInfo { offset: 3600, format: Format::new("CE%sT"), saving: Saving::Multiple("Libya".to_owned()), end_time: Some(ZoneTime::UntilYear(YearSpec::Number(1959)))  },
            ZoneInfo { offset: 7200, format: Format::new("EET"),   saving: Saving::NoSaving,                     end_time: Some(ZoneTime::UntilYear(YearSpec::Number(1982)))   },
            ZoneInfo { offset: 3600, format: Format::new("CE%sT"), saving: Saving::Multiple("Libya".to_owned()), end_time: Some(ZoneTime::UntilDay (YearSpec::Number(1990), MonthSpec(May),       DaySpec::Ordinal( 4)))   },
            ZoneInfo { offset: 7200, format: Format::new("EET"),   saving: Saving::NoSaving,                     end_time: Some(ZoneTime::UntilDay (YearSpec::Number(1996), MonthSpec(September), DaySpec::Ordinal(30)))   },
            ZoneInfo { offset: 3600, format: Format::new("CE%sT"), saving: Saving::Multiple("Libya".to_owned()), end_time: Some(ZoneTime::UntilDay (YearSpec::Number(1997), MonthSpec(October),   DaySpec::Ordinal( 4)))   },
            ZoneInfo { offset: 7200, format: Format::new("EET"),   saving: Saving::NoSaving,                     end_time: Some(ZoneTime::UntilTime(YearSpec::Number(2012), MonthSpec(November),  DaySpec::Ordinal(10), TimeSpec::HoursMinutes(2, 0).with_type(TimeType::Wall)))  },
            ZoneInfo { offset: 3600, format: Format::new("CE%sT"), saving: Saving::Multiple("Libya".to_owned()), end_time: Some(ZoneTime::UntilTime(YearSpec::Number(2013), MonthSpec(October),   DaySpec::Ordinal(25), TimeSpec::HoursMinutes(2, 0).with_type(TimeType::Wall)))  },
            ZoneInfo { offset: 7200, format: Format::new("EET"),   saving: Saving::NoSaving,                     end_time: None              },
        ];

        let mut table = Table::default();
        table.zonesets.insert("Test/Zone".to_owned(), Zoneset(zone));
        table.rulesets.insert("Libya".to_owned(), libya);

        assert_eq!(table.transitions("Test/Zone"), vec![
            Transition { utc_offset: 3164, dst_offset: 0,    occurs_at: None,                 name:  "LMT".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(-1_577_926_364), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(  -574_902_000), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(  -568_087_200), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(  -512_175_600), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(  -504_928_800), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(  -449_888_400), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(  -441_856_800), name:  "CET".to_owned() },
            Transition { utc_offset: 7200, dst_offset: 0,    occurs_at: Some(  -347_158_800), name:  "EET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(   378_684_000), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(   386_463_600), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(   402_271_200), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(   417_999_600), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(   433_807_200), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(   449_622_000), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(   465_429_600), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(   481_590_000), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(   496_965_600), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(   512_953_200), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(   528_674_400), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(   544_230_000), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(   560_037_600), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(   575_852_400), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(   591_660_000), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(   607_388_400), name: "CEST".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(   623_196_000), name:  "CET".to_owned() },
            Transition { utc_offset: 7200, dst_offset: 0,    occurs_at: Some(   641_775_600), name:  "EET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some(   844_034_400), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some(   860_108_400), name: "CEST".to_owned() },
            Transition { utc_offset: 7200, dst_offset: 0,    occurs_at: Some(   875_916_000), name:  "EET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 0,    occurs_at: Some( 1_352_505_600), name:  "CET".to_owned() },
            Transition { utc_offset: 3600, dst_offset: 3600, occurs_at: Some( 1_364_515_200), name: "CEST".to_owned() },
            Transition { utc_offset: 7200, dst_offset: 0,    occurs_at: Some( 1_382_659_200), name:  "EET".to_owned() },
        ]);
    }

    #[test]
    fn optimise_macquarie() {
        let mut transitions = vec![
            Transition { occurs_at: None,              utc_offset:     0, dst_offset:    0, name:  "zzz".to_owned() },
            Transition { occurs_at: Some(-2214259200), utc_offset: 36000, dst_offset:    0, name: "AEST".to_owned() },
            Transition { occurs_at: Some(-1680508800), utc_offset: 36000, dst_offset: 3600, name: "AEDT".to_owned() },
            Transition { occurs_at: Some(-1669892400), utc_offset: 36000, dst_offset: 3600, name: "AEDT".to_owned() },  // gets removed
            Transition { occurs_at: Some(-1665392400), utc_offset: 36000, dst_offset:    0, name: "AEST".to_owned() },
            Transition { occurs_at: Some(-1601719200), utc_offset:     0, dst_offset:    0, name:  "zzz".to_owned() },
            Transition { occurs_at: Some(-687052800),  utc_offset: 36000, dst_offset:    0, name: "AEST".to_owned() },
            Transition { occurs_at: Some(-94730400),   utc_offset: 36000, dst_offset:    0, name: "AEST".to_owned() },  // also gets removed
            Transition { occurs_at: Some(-71136000),   utc_offset: 36000, dst_offset: 3600, name: "AEDT".to_owned() },
            Transition { occurs_at: Some(-55411200),   utc_offset: 36000, dst_offset:    0, name: "AEST".to_owned() },
            Transition { occurs_at: Some(-37267200),   utc_offset: 36000, dst_offset: 3600, name: "AEDT".to_owned() },
            Transition { occurs_at: Some(-25776000),   utc_offset: 36000, dst_offset:    0, name: "AEST".to_owned() },
            Transition { occurs_at: Some(-5817600),    utc_offset: 36000, dst_offset: 3600, name: "AEDT".to_owned() },
        ];

        let mut result = transitions.clone();
        result.remove(7);
        result.remove(3);

        optimise(&mut transitions);
        assert_eq!(transitions, result);
    }

}
