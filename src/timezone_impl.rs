use chrono::{Offset, TimeZone, NaiveDate, NaiveDateTime, LocalResult, Duration};
use std::fmt::{Display, Formatter, Error};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FixedTimespan {
    pub utc_offset: i64,
    pub dst_offset: i64,
    pub name: &'static str,
}

impl Offset for FixedTimespan {
    fn local_minus_utc(&self) -> Duration {
        Duration::seconds(self.utc_offset + self.dst_offset)
    }
}

impl Display for FixedTimespan {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{}", self.name)
    }
}

#[derive(Copy, Clone)]
pub struct FixedTimespanSet {
    pub first: FixedTimespan,
    pub rest: &'static [(i64, FixedTimespan)],
}

pub trait Timespans {
    fn this() -> Self;
    fn timespans() -> FixedTimespanSet;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wrap<T>(pub T);

impl<T: Timespans + Clone> TimeZone for Wrap<T> {
    type Offset = FixedTimespan;

    fn from_offset(_: &Self::Offset) -> Self { Wrap(T::this()) }

    fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<Self::Offset> {
        self.offset_from_local_datetime(&local.and_hms(0, 0, 0))
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<Self::Offset> {
        let timestamp = local.timestamp();
        let timespans = T::timespans();
        for index in 0..timespans.rest.len() {
            let (_, FixedTimespan { utc_offset: utc_offset1, dst_offset: dst_offset1, name: name1 })
                    = if index == 0 { (0, timespans.first) } else { timespans.rest[index - 1] };
            let (start2, FixedTimespan { utc_offset: utc_offset2, dst_offset: dst_offset2, name: name2 })
                                                    = timespans.rest[index];
            let localtime1 = start2 + utc_offset1 + dst_offset1;
            let localtime2 = start2 + utc_offset2 + dst_offset2;
            if localtime1 >= timestamp && localtime2 > timestamp {
                return LocalResult::Single(
                    FixedTimespan {
                        utc_offset: utc_offset1,
                        dst_offset: dst_offset1,
                        name: name1,
                    }
                );
            } else if localtime1 >= timestamp && localtime2 <= timestamp {
                return LocalResult::Ambiguous(
                    FixedTimespan {
                        utc_offset: utc_offset1,
                        dst_offset: dst_offset1,
                        name: name1,
                    },
                    FixedTimespan {
                        utc_offset: utc_offset2,
                        dst_offset: dst_offset2,
                        name: name2,
                    },
                );
            } else if localtime1 < timestamp && localtime2 > timestamp {
                return LocalResult::None
            }
        }
        if timespans.rest.len() > 0 {
            return LocalResult::Single(timespans.rest[timespans.rest.len() - 1].1);
        } else {
            return LocalResult::Single(timespans.first);
        }
    }

    fn offset_from_utc_date(&self, utc: &NaiveDate) -> Self::Offset {
        self.offset_from_utc_datetime(&utc.and_hms(0, 0, 0))
    }

    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> Self::Offset {
        let timestamp = utc.timestamp();
        let timespans = T::timespans();
        let timespan = timespans.rest.iter().rev().find(|&&(start, _)| timestamp >= start);
        if let Some(&(_, timespan)) = timespan {
            timespan
        } else {
            timespans.first
        }
    }
}
