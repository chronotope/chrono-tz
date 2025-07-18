/// This test is compiled by the Github workflows with the
/// filter regex set thusly: CHRONO_TZ_TIMEZONE_FILTER="Europe/(London|Vaduz)"
///
/// We use it to check two things:
/// 1) That the compiled chrono-tz contains the correct timezones (a compilation
///    failure will result if it doesn't).
/// 2) That the compiled chrono-tz DOES NOT contain other, non-matched,
///    timezones. This is rather trickier to do without triggering a compilation
///    failure: we try our best by looking over the TZ_VARIANTS array to try and
///    ascertain if it contains anything obviously wrong.

#[cfg(test)]
mod tests {
    use chrono::offset::TimeZone;
    use chrono_tz::{Europe, Tz, TZ_VARIANTS};
    use std::str::FromStr;

    #[test]
    fn london_compiles() {
        // This line will be a compilation failure if the code generation
        // mistakenly excluded Europe::London.
        let _london_time = Europe::London.with_ymd_and_hms(2013, 12, 25, 14, 0, 0);
        assert_eq!("Europe/London", Europe::London.name());

        // Since London is included, converting from the corresponding
        // string representation should also work.
        assert_eq!(Tz::from_str("Europe/London"), Ok(Europe::London));

        // Vaduz is a link to a zone we didn't ask for, check that it still works.
        assert_eq!(Tz::from_str("Europe/Vaduz"), Ok(Europe::Vaduz));
    }

    #[test]
    fn excluded_things_are_missing() {
        // Timezones from outside Europe should not be included.
        // We can't test all possible strings, here we just handle a
        // representative set.
        assert!(Tz::from_str("Australia/Melbourne").is_err());
        assert!(Tz::from_str("Indian/Maldives").is_err());
        assert!(Tz::from_str("Mexico/BajaSur").is_err());
        assert!(Tz::from_str("Pacific/Kwajalein").is_err());
        assert!(Tz::from_str("US/Central").is_err());

        // Similar for timezones inside Europe, including those that link
        // to London, or that Vaduz links to.
        assert!(Tz::from_str("Europe/Isle_Of_Man").is_err());
        assert!(Tz::from_str("Europe/Belfast").is_err());
        assert!(Tz::from_str("Europe/Zurich").is_err());
        assert!(Tz::from_str("Europe/Brussels").is_err());
        assert!(Tz::from_str("Europe/Brussels").is_err());
        assert!(Tz::from_str("Europe/Dublin").is_err());
        assert!(Tz::from_str("Europe/Warsaw").is_err());

        // Top level zones, including UTC and GMT should also be excluded
        assert!(Tz::from_str("UTC").is_err());
        assert!(Tz::from_str("GMT").is_err());
        assert!(Tz::from_str("EST5EDT").is_err());

        // There should only really be those two zones.
        assert_eq!(TZ_VARIANTS.len(), 2);
    }
}
