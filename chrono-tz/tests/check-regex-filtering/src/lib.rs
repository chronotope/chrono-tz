/// This test is compiled by the Github workflows with the
/// filter regex set thusly: CHRONO_TZ_TIMEZONE_FILTER="(Europe/London|GMT)"
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
    use chrono_tz::{Europe, Europe::London, Tz, TZ_VARIANTS};
    use std::str::FromStr;

    #[test]
    fn london_compiles() {
        // This line will be a compilation failure if the code generation
        // mistakenly excluded Europe::London.
        let _london_time = London.ymd(2013, 12, 25).and_hms(14, 0, 0);
        assert_eq!("Europe/London", London.name());

        // Since London is included, converting from the corresponding
        // string representation should also work.
        assert_eq!(Tz::from_str("Europe/London"), Ok(London));

        // We did not explicitly ask for Isle Of Man or Belfast in our regex, but there is a link
        // from Europe::London to Isle_of_Man and Belfast (amongst others)
        // so these conversions should also work.
        assert_eq!(Tz::from_str("Europe/Isle_of_Man"), Ok(Europe::Isle_of_Man));
        assert_eq!(Tz::from_str("Europe/Belfast"), Ok(Europe::Belfast));
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

        // The link table caused us to include some extra items from the UK (see
        // `london_compiles()`), but it should NOT include various other timezones
        // from around Europe since there is no linkage between them.
        assert!(Tz::from_str("Europe/Brussels").is_err());
        assert!(Tz::from_str("Europe/Dublin").is_err());
        assert!(Tz::from_str("Europe/Warsaw").is_err());

        // Also, entire continents outside Europe should be excluded.
        for tz in TZ_VARIANTS.iter() {
            assert!(!tz.name().starts_with("Africa"));
            assert!(!tz.name().starts_with("Asia"));
            assert!(!tz.name().starts_with("Australia"));
            assert!(!tz.name().starts_with("Canada"));
            assert!(!tz.name().starts_with("Chile"));
            assert!(!tz.name().starts_with("Indian"));
            assert!(!tz.name().starts_with("Mexico"));
            assert!(!tz.name().starts_with("Pacific"));
            assert!(!tz.name().starts_with("US"));
        }
    }
}
