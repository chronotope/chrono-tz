use chrono::{DateTime, Offset, TimeZone};
use std::fs;

#[test]
fn tzfile() {
    let Ok(system_version) = fs::read_to_string("/usr/share/zoneinfo/+VERSION") else {
        return;
    };

    if chrono_tz::IANA_TZDB_VERSION != system_version.trim() {
        return;
    }

    for tz in chrono_tz::TZ_VARIANTS {
        let Ok(file) = tzfile::Tz::named(tz.name()) else {
            continue;
        };

        println!("{}", tz.name());

        for seconds_since_epoch in (0..i32::MAX).step_by(60 * 60) {
            let utc_datetime = DateTime::from_timestamp(seconds_since_epoch as i64, 0)
                .unwrap()
                .naive_utc();

            assert_eq!(
                (&file).offset_from_utc_datetime(&utc_datetime).fix(),
                tz.offset_from_utc_datetime(&utc_datetime).fix(),
                "{seconds_since_epoch} {}",
                tz.name()
            );
        }
    }
}
