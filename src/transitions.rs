use table::{Table, Saving};
use datetime::LocalDateTime;


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


pub trait TableTransitions {
    fn timespans(&self, zone_name: &str) -> FixedTimespanSet;
}


impl TableTransitions for Table {

    /// Computes a fixed timespan set for the timezone with the given name.
    fn timespans(&self, zone_name: &str) -> FixedTimespanSet {
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



#[cfg(test)]
mod test {
    use super::*;
    use super::optimise;

    // Allow unused results in test code, because the only ‘results’ that
    // we need to ignore are the ones from inserting and removing from
    // tables and vectors. And as we set them up ourselves, they’re bound
    // to be correct, otherwise the tests would fail!
    #[test]
    #[allow(unused_results)]
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
