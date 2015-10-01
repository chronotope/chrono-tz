use std::ascii::AsciiExt;
use std::str::FromStr;

use datetime::local;

use regex::{Regex, Captures};


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
    "##).unwrap();

    /// Format of a day specification.
    static ref DAY_FIELD: Regex = Regex::new(r##"(?x) ^
        ( ?P<weekday> \w+ )
        ( ?P<sign>    [<>] = )
        ( ?P<day>     \d+ )
    $ "##).unwrap();

    /// Format of an hour and a minute specification.
    static ref HM_FIELD: Regex = Regex::new(r##"(?x) ^
        ( ?P<hour> -? \d{1,2} ) : ( ?P<minute> \d{2} )
        ( ?P<flag> [wsugz] )?
    $ "##).unwrap();

    /// Format of an hour, a minute, and a second specification.
    static ref HMS_FIELD: Regex = Regex::new(r##"(?x) ^
        ( ?P<hour> -? \d{1,2} ) : ( ?P<minute> \d{2} ) : ( ?P<second> \d{2} )
        ( ?P<flag> [wsugz] )?
    $ "##).unwrap();

    // ^ those two could be done with the same regex, but... they aren't.

    /// Format of a Zone line, with one capturing group per field.
    static ref ZONE_LINE: Regex = Regex::new(r##"(?x) ^
        Zone \s+
        ( ?P<name> [ A-Z a-z 0-9 / _ + - ]+ )  \s+
        ( ?P<gmtoff>     \S+ )  \s+
        ( ?P<rulessave>  \S+ )  \s+
        ( ?P<format>     \S+ )  \s*
        ( ?P<year>       \S+ )? \s*
        ( ?P<month>      \S+ )? \s*
        ( ?P<day>        \S+ )? \s*
        ( ?P<time>       \S+ )?
    "##).unwrap();

    /// Format of a Continuation Zone line, which is the same as the opening
    /// Zone line except the first two fields are replaced by whitespace.
    static ref CONTINUATION_LINE: Regex = Regex::new(r##"(?x) ^
        \s+
        ( ?P<gmtoff>     \S+ )  \s+
        ( ?P<rulessave>  \S+ )  \s+
        ( ?P<format>     \S+ )  \s*
        ( ?P<year>       \S+ )? \s*
        ( ?P<month>      \S+ )? \s*
        ( ?P<day>        \S+ )? \s*
        ( ?P<time>       \S+ )?
    "##).unwrap();

    /// Format of a Link line, with one capturing group per field.
    static ref LINK_LINE: Regex = Regex::new(r##"(?x) ^
        Link  \s+
        ( ?P<target>  \S+ )  \s+
        ( ?P<name>    \S+ )
    "##).unwrap();
}


/// A **rule** definition line.
///
/// According to the `zic(8)` man page, a rule line has this form, along with
/// an example:
///
/// ```text
///     Rule  NAME  FROM  TO    TYPE  IN   ON       AT    SAVE  LETTER/S
///     Rule  US    1967  1973  ‐     Apr  lastSun  2:00  1:00  D
/// ```
///
/// Apart from the opening `Rule` to specify which kind of line this is, and
/// the `type` column, every column in the line has a field in this struct.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct Rule<'line> {

    /// The name of the set of rules that this rule is part of.
    pub name: &'line str,

    /// The first year in which the rule applies.
    pub from_year: YearSpec,

    /// The final year, or `None` if's 'only'.
    pub to_year: Option<YearSpec>,

    /// The month in which the rule takes effect.
    pub month: MonthSpec,

    /// The day on which the rule takes effect.
    pub day: DaySpec,

    /// The time of day at which the rule takes effect.
    pub time: TimeSpecAndType,

    /// The amount of time to be added when the rule is in effect.
    pub time_to_add: TimeSpec,

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

            // The end year can be 'only' to indicate that this rule only
            // takes place on that year.
            let to_year      = match caps.name("to").unwrap() {
                "only"  => None,
                to      => Some(try!(to.parse())),
            };

            // According to the spec, the only value inside the 'type' column
            // should be "-", so throw an error if it isn't. (It only exists
            // for compatibility with old versions that used to contain year
            // types.) Sometimes "‐", a Unicode hyphen, is used as well.
            let t = caps.name("type").unwrap();
            if t != "-" && t != "\u{2010}"  {
                return Err(Error::Fail);
            }

            let month         = try!(caps.name("in").unwrap().parse());
            let day           = try!(caps.name("on").unwrap().parse());
            let time          = try!(caps.name("at").unwrap().parse());
            let time_to_add   = try!(caps.name("save").unwrap().parse());
            let letters       = match caps.name("letters").unwrap() {
                "-"  => None,
                l    => Some(l),
            };

            Ok(Rule {
                name:         name,
                from_year:    from_year,
                to_year:      to_year,
                month:        month,
                day:          day,
                time:         time,
                time_to_add:  time_to_add,
                letters:      letters,
            })
        }
        else {
            Err(Error::Fail)
        }
    }
}

/// A **zone** definition line.
///
/// According to the `zic(8)` man page, a zone line has this form, along with
/// an example:
///
/// ```text
///     Zone  NAME                GMTOFF  RULES/SAVE  FORMAT  [UNTILYEAR [MONTH [DAY [TIME]]]]
///     Zone  Australia/Adelaide  9:30    Aus         AC%sT   1971       Oct    31   2:00
/// ```
///
/// The opening `Zone` identifier is ignored, and the last four columns are
/// all optional, with their variants consolidated into a `ZoneTime`.
///
/// The `Rules/Save` column, if it contains a value, *either* contains the
/// name of the rules to use for this zone, *or* contains a one-off period of
/// time to save.
///
/// A continuation rule line contains all the same fields apart from the
/// `Name` column and the opening `Zone` identifier.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct Zone<'line> {

    /// The name of the time zone.
    pub name: &'line str,

    /// All the other fields of info.
    pub info: ZoneInfo<'line>,
}

/// The information contained in both zone lines *and* zone continuation lines.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct ZoneInfo<'line> {

    /// The amount of time to be added to Universal TimeSpec, to get standard time
    /// in this zone.
    pub gmt_offset: TimeSpec,

    /// The name of all the rules that should apply in the time zone, or the
    /// amount of time to add.
    pub saving: Saving<'line>,

    /// The format for time zone abbreviations, with `%s` as the string marker.
    pub format: &'line str,

    /// The time at which the rules change for this location, or `None` if these rules
    pub time: Option<ZoneTime>,
}

impl<'line> Zone<'line> {
    fn from_str(input: &str) -> Result<Zone, Error> {
        if let Some(caps) = ZONE_LINE.captures(input) {
            let name = caps.name("name").unwrap();
            let info = try!(ZoneInfo::from_captures(caps));

            Ok(Zone {
                name: name,
                info: info,
            })
        }
        else {
            Err(Error::Fail)
        }
    }
}

impl<'line> ZoneInfo<'line> {
    fn from_captures(caps: Captures<'line>) -> Result<ZoneInfo<'line>, Error> {
        let gmt_offset    = try!(caps.name("gmtoff").unwrap().parse());
        let saving        = try!(Saving::from_str(caps.name("rulessave").unwrap()));
        let format        = caps.name("format").unwrap();

        // The year, month, day, and time fields are all optional, meaning
        // that it should be impossible to, say, have a defined month but not
        // a defined year.
        let time = match (caps.name("year"), caps.name("month"), caps.name("day"), caps.name("time")) {
            (Some(y), Some(m), Some(d), Some(t)) => Some(ZoneTime::UntilTime  (try!(y.parse()), try!(m.parse()), try!(d.parse()), try!(t.parse()))),
            (Some(y), Some(m), Some(d), _      ) => Some(ZoneTime::UntilDay   (try!(y.parse()), try!(m.parse()), try!(d.parse()))),
            (Some(y), Some(m), _      , _      ) => Some(ZoneTime::UntilMonth (try!(y.parse()), try!(m.parse()))),
            (Some(y), _      , _      , _      ) => Some(ZoneTime::UntilYear  (try!(y.parse()))),
            (None   , None   , None   , None   ) => None,
            _                                    => unreachable!("Out-of-order capturing groups!"),
        };

        Ok(ZoneInfo {
            gmt_offset:  gmt_offset,
            saving:      saving,
            format:      format,
            time:        time,
        })
    }
}


/// The amount of daylight saving time (DST) to apply to this timespan. This
/// is a special type for a certain field in a zone line, which can hold
/// different types of value.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum Saving<'line> {

    /// Just stick to the base offset.
    NoSaving,

    /// This amount of time should be saved while this timespan is in effect.
    /// (This is the equivalent to there being a single one-off rule with the
    /// given amount of time to save).
    OneOff(TimeSpec),

    /// All rules with the given name should apply while this timespan is in
    /// effect.
    Multiple(&'line str),
}

impl<'line> Saving<'line> {
    fn from_str(input: &str) -> Result<Saving, Error> {
        if input == "-" {
            Ok(Saving::NoSaving)
        }
        else if input.chars().all(|c| c == '-' || c == '_' || c.is_alphabetic()) {
            Ok(Saving::Multiple(input))
        }
        else if HM_FIELD.is_match(input) {
            let time = try!(input.parse());
            Ok(Saving::OneOff(time))
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
    UntilYear(YearSpec),

    /// The earliest point in a particular **month**.
    UntilMonth(YearSpec, MonthSpec),

    /// The earliest point in a particular **day**.
    UntilDay(YearSpec, MonthSpec, DaySpec),

    /// The earliest point in a particular hour, minute, or second, or one of
    /// those small time units, anyway.
    UntilTime(YearSpec, MonthSpec, DaySpec, TimeSpecAndType),
}


/// A **link** definition line.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct Link<'line> {

    /// The target time zone, which should appear as the name in a zone definition.
    pub existing: &'line str,

    /// Another name that the target can be called.
    pub new: &'line str,
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


/// A **year** definition field.
///
/// A year has one of the following representations in a file:
///
/// - `min` or `minimum`, the minimum year possible, for when a rule needs to
///   apply up until the first rule with a specific year;
/// - `max` or `maximum`, the maximum year possible, for when a rule needs to
///   apply after the last rule with a specific year;
/// - a year number, referring to a specific year.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum YearSpec {

    /// The minimum year possible: `min` or `minimum`.
    Minimum,

    /// The maximum year possible: `max` or `maximum`.
    Maximum,

    /// A specific year number.
    Number(i32),
}

impl FromStr for YearSpec {
    type Err = Error;

    fn from_str(input: &str) -> Result<YearSpec, Self::Err> {
        if input == "min" || input == "minimum" {
            Ok(YearSpec::Minimum)
        }
        else if input == "max" || input == "maximum" {
            Ok(YearSpec::Maximum)
        }
        else if input.chars().all(|c| c.is_digit(10)) {
            Ok(YearSpec::Number(input.parse().unwrap()))
        }
        else {
            Err(Error::Fail)
        }
    }
}


/// A **month** field, which is actually just a wrapper around
/// `datetime::local::Month`.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct MonthSpec(local::Month);

impl FromStr for MonthSpec {
    type Err = Error;

    /// Attempts to parse the given string into a value of this type.
    fn from_str(input: &str) -> Result<MonthSpec, Self::Err> {
        Ok(match &*input.to_ascii_lowercase() {
            "jan" | "january"    => MonthSpec(local::Month::January),
            "feb" | "february"   => MonthSpec(local::Month::February),
            "mar" | "march"      => MonthSpec(local::Month::March),
            "apr" | "april"      => MonthSpec(local::Month::April),
            "may"                => MonthSpec(local::Month::May),
            "jun" | "june"       => MonthSpec(local::Month::June),
            "jul" | "july"       => MonthSpec(local::Month::July),
            "aug" | "august"     => MonthSpec(local::Month::August),
            "sep" | "september"  => MonthSpec(local::Month::September),
            "oct" | "october"    => MonthSpec(local::Month::October),
            "nov" | "november"   => MonthSpec(local::Month::November),
            "dec" | "december"   => MonthSpec(local::Month::December),
                  _              => return Err(Error::Fail),
        })
    }
}


/// A **weekday** field, which is actually just a wrapper around
/// `datetime::local::Weekday`.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct WeekdaySpec(local::Weekday);

impl FromStr for WeekdaySpec {
    type Err = Error;

    fn from_str(input: &str) -> Result<WeekdaySpec, Self::Err> {

        Ok(match &*input.to_ascii_lowercase() {
            "mon" | "monday"     => WeekdaySpec(local::Weekday::Monday),
            "tue" | "tuesday"    => WeekdaySpec(local::Weekday::Tuesday),
            "wed" | "wednesday"  => WeekdaySpec(local::Weekday::Wednesday),
            "thu" | "thursday"   => WeekdaySpec(local::Weekday::Thursday),
            "fri" | "friday"     => WeekdaySpec(local::Weekday::Friday),
            "sat" | "saturday"   => WeekdaySpec(local::Weekday::Saturday),
            "sun" | "sunday"     => WeekdaySpec(local::Weekday::Sunday),
                  _              => return Err(Error::Fail),
        })
    }
}


/// A **day** definition field.
///
/// This can be given in either absolute terms (such as "the fifth day of the
/// month"), or relative terms (such as "the last Sunday of the month", or
/// "the last Friday before or including the 13th").
///
/// Note that in the last example, it's allowed for that particular Friday to
/// *be* the 13th in question.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum DaySpec {

    /// A specific day of the month, given by its number.
    Ordinal(i32),

    /// The last day of the month with a specific weekday.
    Last(WeekdaySpec),

    /// The **last** day with the given weekday **before** (or including) a
    /// day with a specific number.
    LastOnOrBefore(WeekdaySpec, i32),

    /// The **first** day with the given weekday **after** (or including) a
    /// day with a specific number.
    FirstOnOrAfter(WeekdaySpec, i32)
}

impl FromStr for DaySpec {
    type Err = Error;

    fn from_str(input: &str) -> Result<DaySpec, Self::Err> {

        // Parse the field as a number if it vaguely resembles one.
        if input.chars().all(|c| c.is_digit(10)) {
            Ok(DaySpec::Ordinal(input.parse().unwrap()))
        }

        // Check if it stars with 'last', and trim off the first four bytes if
        // it does. (Luckily, the file is ASCII.)
        else if input.starts_with("last") {
            let weekday = try!(input[4..].parse());
            Ok(DaySpec::Last(weekday))
        }

        // Check if it's a relative expression with the regex.
        else if let Some(caps) = DAY_FIELD.captures(input) {
            let weekday = caps.name("weekday").unwrap().parse().unwrap();
            let day     = caps.name("day").unwrap().parse().unwrap();

            match caps.name("sign").unwrap() {
                "<=" => Ok(DaySpec::LastOnOrBefore(weekday, day)),
                ">=" => Ok(DaySpec::FirstOnOrAfter(weekday, day)),
                 _   => unreachable!("The regex only matches one of those two!"),
            }
        }

        // Otherwise, give up.
        else {
            Err(Error::Fail)
        }
    }
}


/// A **time** definition field.
///
/// A time must have an hours component, with optional minutes and seconds
/// components. It can also be negative with a starting '-'.
///
/// Hour 0 is midnight at the start of the day, and Hour 24 is midnight at the
/// end of the day.
///
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum TimeSpec {

    /// A number of hours.
    Hours(i32),

    /// A number of hours and minutes.
    HoursMinutes(i32, i32),

    /// A number of hours, minutes, and seconds.
    HoursMinutesSeconds(i32, i32, i32),

    /// Zero, or midnight at the start of the day.
    Zero,
}

/// The "type" of time that a time is.
///
/// A time may be followed with a letter, signifying what 'type'
/// of time the timestamp is:
///
/// - **w** for "wall clock" time (the default),
/// - **s** for local standard time,jkdx
/// - **u** or **g** or **z** for universal time.
#[derive(PartialEq, Debug, Copy, Clone)]
pub enum TimeType {

    /// Wall-clock time.
    Wall,

    /// Standard Time.
    Standard,

    /// Universal Co-ordinated Time.
    UTC,
}

/// A time spec *and* a time type. Certain fields need to have both.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct TimeSpecAndType(pub TimeSpec, pub TimeType);

#[cfg(test)]
impl TimeSpec {
    fn with_type(self, time_type: TimeType) -> TimeSpecAndType {
        TimeSpecAndType(self, time_type)
    }
}

impl FromStr for TimeSpecAndType {
    type Err = Error;

    fn from_str(input: &str) -> Result<TimeSpecAndType, Self::Err> {
        if input == "-" {
            Ok(TimeSpecAndType(TimeSpec::Zero, TimeType::Wall))
        }
        else if input.chars().all(|c| c == '-' || c.is_digit(10)) {
            Ok(TimeSpecAndType(TimeSpec::Hours(input.parse().unwrap()), TimeType::Wall))
        }
        else if let Some(caps) = HM_FIELD.captures(input) {
            let hour   = caps.name("hour").unwrap().parse().unwrap();
            let minute = caps.name("minute").unwrap().parse().unwrap();
            let flag   = caps.name("flag").and_then(|c| TimeType::from_str(&c[0..1]))
                                          .unwrap_or(TimeType::Wall);

            Ok(TimeSpecAndType(TimeSpec::HoursMinutes(hour, minute), flag))
        }
        else if let Some(caps) = HMS_FIELD.captures(input) {
            let hour   = caps.name("hour").unwrap().parse().unwrap();
            let minute = caps.name("minute").unwrap().parse().unwrap();
            let second = caps.name("second").unwrap().parse().unwrap();
            let flag   = caps.name("flag").and_then(|c| TimeType::from_str(&c[0..1]))
                                          .unwrap_or(TimeType::Wall);

            Ok(TimeSpecAndType(TimeSpec::HoursMinutesSeconds(hour, minute, second), flag))
        }
        else {
            Err(Error::Fail)
        }
    }
}

impl FromStr for TimeSpec {
    type Err = Error;

    fn from_str(input: &str) -> Result<TimeSpec, Self::Err> {
        match input.parse() {
            Ok(TimeSpecAndType(spec, TimeType::Wall)) => Ok(spec),
            Ok(TimeSpecAndType(_   , _             )) => Err(Error::Fail),
            Err(e)                                    => Err(e),
        }
    }
}

impl TimeType {
    fn from_str(c: &str) -> Option<TimeType> {
        Some(match c {
            "w"             => TimeType::Wall,
            "s"             => TimeType::Standard,
            "u" | "g" | "z" => TimeType::UTC,
             _              => return None,
        })
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

    /// This line is empty.
    Space,

    /// This line contains a **zone** definition.
    Zone(Zone<'line>),

    /// This line contains a **continuation** of a zone definition.
    Continuation(ZoneInfo<'line>),

    /// This line contains a **rule** definition.
    Rule(Rule<'line>),

    /// This line contains a **link** definition.
    Link(Link<'line>),
}

impl<'line> Line<'line> {

    /// Attempts to parse the given string into a value of this type.
    pub fn from_str(input: &str) -> Result<Line, Error> {
        if input.is_empty() || input.chars().all(char::is_whitespace) {
            Ok(Line::Space)
        }
        else if let Ok(zone) = Zone::from_str(input) {
            Ok(Line::Zone(zone))
        }
        else if let Some(caps) = CONTINUATION_LINE.captures(input) {
            Ok(Line::Continuation(try!(ZoneInfo::from_captures(caps))))
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
    pub use std::str::FromStr;
    pub use super::*;
    pub use datetime::local;

    macro_rules! test {
        ($name:ident: $input:expr => $result:expr) => {
            #[test]
            fn $name() {
                assert_eq!(Line::from_str($input), $result);
            }
        };
    }

    test!(empty:    ""          => Ok(Line::Space));
    test!(spaces:   "        "  => Ok(Line::Space));

    mod rules {
        use super::*;

        test!(rule_1: "Rule  US    1967  1973  ‐     Apr  lastSun  2:00  1:00  D" => Ok(Line::Rule(Rule {
            name:         "US",
            from_year:    YearSpec::Number(1967),
            to_year:      Some(YearSpec::Number(1973)),
            month:        MonthSpec(local::Month::April),
            day:          DaySpec::Last(WeekdaySpec(local::Weekday::Sunday)),
            time:         TimeSpec::HoursMinutes(2, 0).with_type(TimeType::Wall),
            time_to_add:  TimeSpec::HoursMinutes(1, 0),
            letters:      Some("D"),
        })));

        test!(rule_2: "Rule	Greece	1976	only	-	Oct	10	2:00s	0	-" => Ok(Line::Rule(Rule {
            name:         "Greece",
            from_year:    YearSpec::Number(1976),
            to_year:      None,
            month:        MonthSpec(local::Month::October),
            day:          DaySpec::Ordinal(10),
            time:         TimeSpec::HoursMinutes(2, 0).with_type(TimeType::Standard),
            time_to_add:  TimeSpec::Hours(0),
            letters:      None,
        })));

        test!(rule_3: "Rule	EU	1977	1980	-	Apr	Sun>=1	 1:00u	1:00	S" => Ok(Line::Rule(Rule {
            name:        "EU",
            from_year:    YearSpec::Number(1977),
            to_year:      Some(YearSpec::Number(1980)),
            month:        MonthSpec(local::Month::April),
            day:          DaySpec::FirstOnOrAfter(WeekdaySpec(local::Weekday::Sunday), 1),
            time:         TimeSpec::HoursMinutes(1, 0).with_type(TimeType::UTC),
            time_to_add:  TimeSpec::HoursMinutes(1, 0),
            letters:      Some("S"),
        })));

        test!(no_hyphen: "Rule	EU	1977	1980	HEY	Apr	Sun>=1	 1:00u	1:00	S"         => Err(Error::Fail));
        test!(bad_month: "Rule	EU	1977	1980	-	Febtober	Sun>=1	 1:00u	1:00	S" => Err(Error::Fail));
    }

    mod zones {
        use super::*;

        test!(zone: "Zone  Australia/Adelaide  9:30    Aus         AC%sT   1971 Oct 31  2:00:00" => Ok(Line::Zone(Zone {
            name: "Australia/Adelaide",
            info: ZoneInfo {
                gmt_offset:  TimeSpec::HoursMinutes(9, 30),
                saving:      Saving::Multiple("Aus"),
                format:      "AC%sT",
                time:        Some(ZoneTime::UntilTime(YearSpec::Number(1971), MonthSpec(local::Month::October), DaySpec::Ordinal(31), TimeSpec::HoursMinutesSeconds(2, 0, 0))),
            },
        })));

        test!(continuation_1: "                          9:30    Aus         AC%sT   1971 Oct 31  2:00:00" => Ok(Line::Continuation(ZoneInfo {
            gmt_offset:  TimeSpec::HoursMinutes(9, 30),
            saving:      Saving::Multiple("Aus"),
            format:      "AC%sT",
            time:        Some(ZoneTime::UntilTime(YearSpec::Number(1971), MonthSpec(local::Month::October), DaySpec::Ordinal(31), TimeSpec::HoursMinutesSeconds(2, 0, 0))),
        })));

        test!(continuation_2: "			1:00	C-Eur	CE%sT	1943 Oct 25" => Ok(Line::Continuation(ZoneInfo {
            gmt_offset:  TimeSpec::HoursMinutes(1, 00),
            saving:      Saving::Multiple("C-Eur"),
            format:      "CE%sT",
            time:        Some(ZoneTime::UntilDay(YearSpec::Number(1943), MonthSpec(local::Month::October), DaySpec::Ordinal(25))),
        })));

        test!(zone_hyphen: "Zone Asia/Ust-Nera\t 9:32:54 -\tLMT\t1919" => Ok(Line::Zone(Zone {
            name: "Asia/Ust-Nera",
            info: ZoneInfo {
                gmt_offset:  TimeSpec::HoursMinutesSeconds(9, 32, 54),
                saving:      Saving::NoSaving,
                format:      "LMT",
                time:        Some(ZoneTime::UntilYear(YearSpec::Number(1919))),
            },
        })));
    }

    test!(link: "Link  Europe/Istanbul  Asia/Istanbul" => Ok(Line::Link(Link {
        existing:  "Europe/Istanbul",
        new:       "Asia/Istanbul",
    })));

    #[test]
    fn month() {
        assert_eq!(MonthSpec::from_str("Aug"), Ok(MonthSpec(local::Month::August)));
        assert_eq!(MonthSpec::from_str("December"), Ok(MonthSpec(local::Month::December)));
    }

    test!(golb: "GOLB" => Err(Error::Fail));
}
