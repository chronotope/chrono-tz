use std::collections::hash_map::{HashMap, Entry};

use line::{self, YearSpec, DaySpec, MonthSpec, ZoneTime};


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

#[derive(PartialEq, Debug)]
struct RuleInfo {

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
            time_to_add:  info.time_to_add.as_seconds(),
            letters:      info.letters.map(str::to_owned),
        }
    }
}

#[derive(PartialEq, Debug, Default)]
pub struct Zoneset(pub Vec<ZoneInfo>);

#[derive(PartialEq, Debug)]
pub struct ZoneInfo {
    pub gmt_offset: i64,
    pub format:     String,
    pub saving:     Saving,
    pub until:      Option<ZoneTime>,
}

#[derive(PartialEq, Debug)]
pub enum Saving {
    NoSaving,
    OneOff(i64),
    Multiple(String),
}

impl<'_> From<line::ZoneInfo<'_>> for ZoneInfo {
    fn from(info: line::ZoneInfo) -> ZoneInfo {
        ZoneInfo {
            gmt_offset: info.gmt_offset.as_seconds(),
            saving: match info.saving {
                line::Saving::NoSaving     => Saving::NoSaving,
                line::Saving::Multiple(s)  => Saving::Multiple(s.to_owned()),
                line::Saving::OneOff(t)    => Saving::OneOff(t.as_seconds()),
            },
            format: info.format.to_owned(),
            until: info.time,
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