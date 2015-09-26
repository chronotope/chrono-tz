//! Parsing Olson DB formats.

#![warn(missing_copy_implementations)]
#![warn(missing_debug_implementations)]
#![warn(missing_docs)]

#![warn(trivial_casts, trivial_numeric_casts)]
#![warn(unused_qualifications)]
#![warn(unused_results)]

use std::ascii::AsciiExt;
use std::str::FromStr;

extern crate datetime;
use datetime::local;

extern crate regex;
use regex::Regex;

#[macro_use]
extern crate lazy_static;


/// A set of regexes to test against.
///
/// All of these regexes use the `(?x)` flag, which means they support
/// comments and whitespace directly in the regex string!
lazy_static! {

    /// Format of a Rule line: one capturing group per field.
    static ref RULE_LINE: Regex = Regex::new(r##"(?x) ^
        Rule \s+
        ( ?P<name>    \S+)  \s+
        ( ?P<from>    \S+)  \s+
        ( ?P<to>      \S+)  \s+
        ( ?P<type>    \S+)  \s+
        ( ?P<in>      \S+)  \s+
        ( ?P<on>      \S+)  \s+
        ( ?P<at>      \S+)  \s+
        ( ?P<save>    \S+)  \s+
        ( ?P<letters> \S+)
    $ "##).unwrap();

    /// Format of a Day specification in a Rule.
    static ref DAY_FIELD: Regex = Regex::new(r##"(?x) ^
        ( ?P<weekday> \w+ )
        ( ?P<sign>    [<>] = )
        ( ?P<day>     \d+ )
    $ "##).unwrap();

    /// Format of an hour and a minute.
    static ref HM_FIELD: Regex = Regex::new(r##"(?x) ^
        ( ?P<hour> \d{1,2} ) : ( ?P<minute> \d{2} )
        ( ?P<flag> [wsugz] )?
    $ "##).unwrap();

    /// Format of an hour, a minute, and a second.
    static ref HMS_FIELD: Regex = Regex::new(r##"(?x) ^
        ( ?P<hour> \d{1,2} ) : ( ?P<minute> \d{2} ) : ( ?P<second> \d{2} )
        ( ?P<flag> [wsugz] )?
    $ "##).unwrap();

    /// Format of a Zone line, with one capturing group per field.
    static ref ZONE_LINE: Regex = Regex::new(r##"(?x) ^
        Zone \s+
        ( ?P<name> [ A-Z a-z / ]+ )  \s+
        ( ?P<gmtoff>     \S+ )  \s+
        ( ?P<rulessave>  \S+ )  \s+
        ( ?P<format>     \S+ )  \s*
        ( ?P<year>       \S+ )? \s*
        ( ?P<month>      \S+ )? \s*
        ( ?P<day>        \S+ )? \s*
        ( ?P<time>       \S+ )?
    $ "##).unwrap();

    /// Format of a Link line, with one capturing group per field.
    static ref LINK_LINE: Regex = Regex::new(r##"(?x) ^
        Link  \s+
        ( ?P<target>  \S+ )  \s+
        ( ?P<name>    \S+ )
    $ "##).unwrap();
}


/// A **rule** definition line.
///
/// According to the `zic` man page, a rule line has the form:
///
/// A rule line has the form:
///
/// ```text
///         Rule  NAME  FROM  TO    TYPE  IN   ON       AT    SAVE  LETTER/S
///    For example:
///         Rule  US    1967  1973  ‐     Apr  lastSun  2:00  1:00  D
/// ```
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct Rule<'line> {

    /// The name of the set of rules that this rule is part of.
    pub name: &'line str,

    /// The first year in which the rule applies.
    pub from_year: Year,

    /// The final year, or `None` if's 'only'.
    pub to_year: Option<Year>,

    /// The month in which the rule takes effect.
    pub month: Month,

    /// The day on which the rule takes effect.
    pub day: Day,

    /// The time of day at which the rule takes effect.
    pub time_of_day: Time,

    /// The amount of time to be added when the rule is in effect.
    pub time_to_add: Time,

    /// The variable part of time zone abbreviations to be used when this rule
    /// is in effect, if any.
    pub letters: Option<&'line str>,
}

impl<'line> Rule<'line> {

    /// Attempts to parse the given string into a value of this type.
    fn from_str(input: &str) -> Result<Rule, Error> {
        if let Some(caps) = RULE_LINE.captures(input) {
            let name         = caps.name("name").unwrap();
            let from_year    = try!(caps.name("from").unwrap().parse());
            let to_year      = match caps.name("to").unwrap() {
                "only"  => None,
                to      => Some(try!(to.parse())),
            };

            // According to the spec, the only value inside the 'type' column
            // should be "-", so throw an error if it isn't.
            let t = caps.name("type").unwrap();
            if t != "-" && t != "\u{2010}"  {
                return Err(Error::Fail);
            }

            let month         = try!(caps.name("in").unwrap().parse());
            let day           = try!(caps.name("on").unwrap().parse());
            let time_of_day   = try!(caps.name("at").unwrap().parse());
            let time_to_add   = try!(caps.name("save").unwrap().parse());
            let letters       = match caps.name("letters").unwrap() {
                "-"  => None,
                l    => Some(l),
            };

            Ok(Rule {
                name: name,
                from_year:    from_year,
                to_year:      to_year,
                month:        month,
                day:          day,
                time_of_day:  time_of_day,
                time_to_add:  time_to_add,
                letters:      letters,
            })
        }
        else {
            Err(Error::Fail)
        }
    }
}


/// A **year** definition field.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Year {

    /// The minimum year possible: `min` or `minimum` in a file.
    Minimum,

    /// The maximum year possible: `max` or `maximum` in a file.
    Maximum,

    /// A specific year number.
    Number(i32),
}

impl FromStr for Year {
    type Err = Error;

    fn from_str(input: &str) -> Result<Year, Self::Err> {
        if input == "min" || input == "minimum" {
            Ok(Year::Minimum)
        }
        else if input == "max" || input == "maximum" {
            Ok(Year::Maximum)
        }
        else if input.chars().all(|c| c.is_digit(10)) {
            Ok(Year::Number(input.parse().unwrap()))
        }
        else {
            Err(Error::Fail)
        }
    }
}


/// A **month** field, which is actually just a wrapper around
/// `datetime::local::Month`.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct Month(local::Month);

impl FromStr for Month {
    type Err = Error;

    /// Attempts to parse the given string into a value of this type.
    fn from_str(input: &str) -> Result<Month, Self::Err> {
        Ok(match &*input.to_ascii_lowercase() {
            "jan" | "january"    => Month(local::Month::January),
            "feb" | "february"   => Month(local::Month::February),
            "mar" | "march"      => Month(local::Month::March),
            "apr" | "april"      => Month(local::Month::April),
            "may"                => Month(local::Month::May),
            "jun" | "june"       => Month(local::Month::June),
            "jul" | "july"       => Month(local::Month::July),
            "aug" | "august"     => Month(local::Month::August),
            "sep" | "september"  => Month(local::Month::September),
            "oct" | "october"    => Month(local::Month::October),
            "nov" | "november"   => Month(local::Month::November),
            "dec" | "december"   => Month(local::Month::December),
                  _              => return Err(Error::Fail),
        })
    }
}


/// A **weekday** field, which is actually just a wrapper around
/// `datetime::local::Weekday`.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct Weekday(local::Weekday);

impl FromStr for Weekday {
    type Err = Error;

    fn from_str(input: &str) -> Result<Weekday, Self::Err> {

        Ok(match &*input.to_ascii_lowercase() {
            "mon" | "monday"     => Weekday(local::Weekday::Monday),
            "tue" | "tuesday"    => Weekday(local::Weekday::Tuesday),
            "wed" | "wednesday"  => Weekday(local::Weekday::Wednesday),
            "thu" | "thursday"   => Weekday(local::Weekday::Thursday),
            "fri" | "friday"     => Weekday(local::Weekday::Friday),
            "sat" | "saturday"   => Weekday(local::Weekday::Saturday),
            "sun" | "sunday"     => Weekday(local::Weekday::Sunday),
                  _              => return Err(Error::Fail),
        })
    }
}


/// A **day** definition field.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Day {

    /// A specific day of the month.
    Ordinal(i32),

    /// The last day of the month with a specific weekday.
    Last(Weekday),

    /// The last day **before** a point with a specific weekday.
    LastOnOrBefore(Weekday, i32),

    /// The last day **after** a point with a specific weekday.
    LastOnOrAfter(Weekday, i32)
}

impl FromStr for Day {
    type Err = Error;

    fn from_str(input: &str) -> Result<Day, Self::Err> {
        if input.chars().all(|c| c.is_digit(10)) {
            Ok(Day::Ordinal(input.parse().unwrap()))
        }
        else if input.starts_with("last") {
            let weekday = try!(input[4..].parse());
            Ok(Day::Last(weekday))
        }
        else if let Some(caps) = DAY_FIELD.captures(input) {
            let weekday = caps.name("weekday").unwrap().parse().unwrap();
            let day     = caps.name("day").unwrap().parse().unwrap();

            match caps.name("sign").unwrap() {
                "<" => Ok(Day::LastOnOrBefore(weekday, day)),
                ">" => Ok(Day::LastOnOrAfter(weekday, day)),
                 _  => unreachable!("The regex only matches one of those two"),
            }
        }
        else {
            Err(Error::Fail)
        }
    }
}


/// A **time** definition field.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Time {

    /// A number of hours.
    Hours(i32),

    /// A number of hours and minutes.
    HoursMinutes(i32, i32, Option<char>),

    /// A number of hours, minutes, and seconds.
    HoursMinutesSeconds(i32, i32, i32, Option<char>),

    /// Zero, or midnight at the start of the day.
    Zero,
}

impl FromStr for Time {
    type Err = Error;

    fn from_str(input: &str) -> Result<Time, Self::Err> {

        if input == "-" {
            Ok(Time::Zero)
        }
        else if input.chars().all(|c| c.is_digit(10)) {
            Ok(Time::Hours(input.parse().unwrap()))
        }
        else if let Some(caps) = HM_FIELD.captures(input) {
            let hour   = caps.name("hour").unwrap().parse().unwrap();
            let minute = caps.name("minute").unwrap().parse().unwrap();
            let flag   = caps.name("flag").map(|f| f.chars().next().unwrap());
            Ok(Time::HoursMinutes(hour, minute, flag))
        }
        else if let Some(caps) = HMS_FIELD.captures(input) {
            let hour   = caps.name("hour").unwrap().parse().unwrap();
            let minute = caps.name("minute").unwrap().parse().unwrap();
            let second = caps.name("second").unwrap().parse().unwrap();
            let flag   = caps.name("flag").map(|f| f.chars().next().unwrap());
            Ok(Time::HoursMinutesSeconds(hour, minute, second, flag))
        }
        else {
            Err(Error::Fail)
        }
    }
}


/// A **zone** definition line.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct Zone<'line> {

    /// The name of the time zone.
    pub name: &'line str,

    /// The amount of time to be added to Universal Time, to get standard time
    /// in this zone.
    pub gmt_offset: Time,

    /// The name of all the rules that should apply in the time zone, or the
    /// amount of time to add.
    pub rules_save: RulesSave<'line>,

    /// The format for time zone abbreviations, with `%s` as the string marker.
    pub format: &'line str,

    /// The time at which the rules change for this location, or `None` if these rules
    pub time: Option<ZoneTime>,
}

impl<'line> Zone<'line> {
    fn from_str(input: &str) -> Result<Zone, Error> {
        if let Some(caps) = ZONE_LINE.captures(input) {
            let name          = caps.name("name").unwrap();
            let gmt_offset    = try!(caps.name("gmtoff").unwrap().parse());
            let rules_save    = try!(RulesSave::from_str(caps.name("rulessave").unwrap()));
            let format        = caps.name("format").unwrap();

            let time = match (caps.name("year"), caps.name("month"), caps.name("day"), caps.name("time")) {
                (Some(y), Some(m), Some(d), Some(t)) => Some(ZoneTime::UntilTime  (try!(y.parse()), try!(m.parse()), try!(d.parse()), try!(t.parse()))),
                (Some(y), Some(m), Some(d), _      ) => Some(ZoneTime::UntilDay   (try!(y.parse()), try!(m.parse()), try!(d.parse()))),
                (Some(y), Some(m), _      , _      ) => Some(ZoneTime::UntilMonth (try!(y.parse()), try!(m.parse()))),
                (Some(y), _      , _      , _      ) => Some(ZoneTime::UntilYear  (try!(y.parse()))),
                (None   , None   , None   , None   ) => None,
                _                                    => return Err(Error::Fail),
            };

            Ok(Zone {
                name:        name,
                gmt_offset:  gmt_offset,
                rules_save:  rules_save,
                format:      format,
                time:        time,
            })
        }
        else {
            Err(Error::Fail)
        }
    }
}


/// A specific type for a certain Zone column.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum RulesSave<'line> {

    /// The name of the **rules** to apply.
    Rules(&'line str),

    /// The amount of **save** time to add.
    Save(Time),
}

impl<'line> RulesSave<'line> {
    fn from_str(input: &str) -> Result<RulesSave, Error> {
        if input.chars().all(char::is_alphabetic) {
            Ok(RulesSave::Rules(input))
        }
        else if HM_FIELD.is_match(input) {
            let time = try!(input.parse());
            Ok(RulesSave::Save(time))
        }
        else {
            Err(Error::Fail)
        }
    }
}


/// The time at which the rules change for a location, with varying degrees of precision.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum ZoneTime {

    /// The earliest point in a particular **year**.
    UntilYear(Year),

    /// The earliest point in a particular **month**.
    UntilMonth(Year, Month),

    /// The earliest point in a particular **day**.
    UntilDay(Year, Month, Day),

    /// The earliest point in a particular hour, minute, or second, or one of
    /// those small time units, anyway.
    UntilTime(Year, Month, Day, Time),
}


/// A **link** definition line.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct Link<'line> {

    /// The target time zone, which should appear as the name in a zone definition.
    existing: &'line str,

    /// Another name that the target can be called.
    new: &'line str,
}

impl<'line> Link<'line> {
    fn from_str(input: &str) -> Result<Link, Error> {
        if let Some(caps) = LINK_LINE.captures(input) {
            let target  = caps.name("target").unwrap();
            let name    = caps.name("name").unwrap();
            Ok(Link { existing: target, new: name })
        }
        else {
            Err(Error::Fail)
        }
    }
}


/// An error that can occur during parsing.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Error {

    /// TODO: more error types
    Fail
}

/// A type of valid line that has been parsed.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Line<'line> {

    /// This line is a comment.
    Comment,

    /// This line is empty.
    Space,

    /// This line contains a **zone** definition.
    Zone(Zone<'line>),

    /// This line contains a **rule** definition.
    Rule(Rule<'line>),

    /// This line contains a **link** definition.
    Link(Link<'line>),
}

impl<'line> Line<'line> {

    /// Attempts to parse the given string into a value of this type.
    pub fn from_str(input: &str) -> Result<Line, Error> {
        if input.starts_with("#") {
            Ok(Line::Comment)
        }
        else if input.is_empty() {
            Ok(Line::Space)
        }
        else if let Ok(zone) = Zone::from_str(input) {
            Ok(Line::Zone(zone))
        }
        else if let Ok(rule) = Rule::from_str(input) {
            Ok(Line::Rule(rule))
        }
        else if let Ok(link) = Link::from_str(input) {
            Ok(Line::Link(link))
        }
        else {
            Err(Error::Fail)
        }
    }
}


#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::*;
    use datetime::local;

    macro_rules! test {
        ($name:ident: $input:expr => $result:expr) => {
            #[test]
            fn $name() {
                assert_eq!(Line::from_str($input), Ok($result));
            }
        };
    }

    test!(comment:  "# comment"  => Line::Comment);
    test!(empty:    ""           => Line::Space);

    test!(example: "Rule  US    1967  1973  ‐     Apr  lastSun  2:00  1:00  D" => Line::Rule(Rule {
        name:         "US",
        from_year:    Year::Number(1967),
        to_year:      Some(Year::Number(1973)),
        month:        Month(local::Month::April),
        day:          Day::Last(Weekday(local::Weekday::Sunday)),
        time_of_day:  Time::HoursMinutes(2, 0, None),
        time_to_add:  Time::HoursMinutes(1, 0, None),
        letters:      Some("D"),
    }));

    test!(example_2: "Rule	Greece	1976	only	-	Oct	10	2:00s	0	-" => Line::Rule(Rule {
        name:         "Greece",
        from_year:    Year::Number(1976),
        to_year:      None,
        month:        Month(local::Month::October),
        day:          Day::Ordinal(10),
        time_of_day:  Time::HoursMinutes(2, 0, Some('s')),
        time_to_add:  Time::Hours(0),
        letters:      None,
    }));

    test!(zone: "Zone  Australia/Adelaide  9:30    Aus         AC%sT   1971 Oct 31  2:00" => Line::Zone(Zone {
        name:        "Australia/Adelaide",
        gmt_offset:  Time::HoursMinutes(9, 30, None),
        rules_save:  RulesSave::Rules("Aus"),
        format:      "AC%sT",
        time:        Some(ZoneTime::UntilTime(Year::Number(1971), Month(local::Month::October), Day::Ordinal(31), Time::HoursMinutes(2, 0, None))),
    }));

    test!(link: "Link  Europe/Istanbul  Asia/Istanbul" => Line::Link(Link {
        existing:  "Europe/Istanbul",
        new:       "Asia/Istanbul",
    }));

    #[test]
    fn month() {
        assert_eq!(Month::from_str("Aug"), Ok(Month(local::Month::August)));
    }
}
