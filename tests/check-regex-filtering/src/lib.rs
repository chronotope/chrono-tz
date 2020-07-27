/// This test is compiled by the Github workflows with the
/// filter regex set thusly: CHRONO_TZ_BUILD_TIMEZONES="(Europe/London|GMT)"
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
    use chrono_tz::{Europe::London, Tz, TZ_VARIANTS};
    use std::str::FromStr;

    #[test]
    fn london_compiles() {
        let _london_time = London.ymd(2013, 12, 25).and_hms(14, 0, 0);
        assert_eq!("Europe/London", London.name());
        assert_eq!(Tz::from_str("Europe/London"), Ok(London));
    }

    #[test]
    fn excluded_things_are_missing() {
        assert!(Tz::from_str("Australia/Melbourne").is_err());
        assert!(Tz::from_str("Indian/Maldives").is_err());
        assert!(Tz::from_str("Mexico/BajaSur").is_err());
        assert!(Tz::from_str("Pacific/Kwajalein").is_err());
        assert!(Tz::from_str("US/Central").is_err());

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
