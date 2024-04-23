# parse-zoneinfo

Rust library for reading the text files comprising the [zoneinfo database], which records time zone changes and offsets across the world from multiple sources.

The zoneinfo database is distributed in one of two formats: a raw text format with one file per continent, and a compiled binary format with one file per time zone. This crate deals with the text format.

The database itself is maintained by IANA. For more information, see [IANA’s page on the time zone database][iana]. You can also find the text files in [the tz repository][tz].

Parse-zoneinfo is a fork of [`zoneinfo_parse`] by Benjamin Sago (now unmaintained). It is used by [`chrono-tz`].

[iana]: https://www.iana.org/time-zones
[tz]: https://github.com/eggert/tz
[zoneinfo database]: https://en.wikipedia.org/wiki/Tz_database
[`zoneinfo_parse`]: https://crates.io/crates/zoneinfo_parse
[`chrono-tz`]: https://crates.io/crates/chrono-tz

### [View the Rustdoc](https://docs.rs/parse-zoneinfo)

# Installation

This crate works with [Cargo](https://crates.io). Add the following to your `Cargo.toml` dependencies section:

```toml
[dependencies]
parse-zoneinfo = "0.5"
```

The earliest version of Rust that this crate is tested against is [Rust v1.31.0](https://blog.rust-lang.org/2018/12/06/Rust-1.31-and-rust-2018.html).

# Usage

The zoneinfo files contains `Zone`, `Rule`, and `Link` information. Each type of line forms a variant in the `line::Line` enum.

To get started, here are a few lines representing what time is like in the `Europe/Madrid` time zone:

    # Zone      NAME            GMTOFF  RULES   FORMAT  [UNTIL]
    Zone        Europe/Madrid   -0:14:44 -      LMT     1901 Jan  1  0:00s
                                 0:00   Spain   WE%sT   1946 Sep 30
                                 1:00   Spain   CE%sT   1979
                                 1:00   EU      CE%sT

The first line is a comment. The second starts with `Zone`, so we know

So parsing these five lines would return the five following results:

- A `line::Line::Space` for the comment, because the line doesn’t contain any information (but isn’t strictly *invalid* either).
- A `line::Line::Zone` for the first `Zone` entry. This contains a `Zone` struct that holds the name of the zone. All the other fields are stored in the `ZoneInfo` struct.
- A `line::Line::Continuation` for the next entry. This is different from the line above as it doesn’t contain a name field; it only has the information in a `ZoneInfo` struct.
- The fourth line contains the same types of data as the third.
- As does the fifth.

Lines with rule definitions look like this:

    # Rule      NAME    FROM    TO      TYPE    IN      ON      AT      SAVE    LETTER/S
    Rule        Spain   1917    only    -       May      5      23:00s  1:00    S
    Rule        Spain   1917    1919    -       Oct      6      23:00s  0       -
    Rule        Spain   1918    only    -       Apr     15      23:00s  1:00    S
    Rule        Spain   1919    only    -       Apr      5      23:00s  1:00    S

All these lines follow the same pattern: A `line::Line::Rule` that contains a `Rule` struct, which has a field for each column of data.

Finally, there are lines that link one zone to another’s name:

    Link   Europe/Prague   Europe/Bratislava

The `Link` struct simply contains the names of both the existing and new time zones.


## Interpretation

Once the input lines have been parsed, they must be *interpreted* to form a table of time zone data.

The easiest way to do this is with a `TableBuilder`. You can add various lines to the builder, and it will throw an error as soon as it detects that something’s wrong, such as a duplicate or a missing entry. When all the lines have been fed to the builder, you can use the `build` method to produce a `Table` containing fields for the rule, zone, and link lines.



## Example program

This crate is used to produce the data for the [`zoneinfo-data` crate](https://github.com/rust-datetime/zoneinfo-data). For an example of its use, see the bundled [data crate builder](https://github.com/rust-datetime/zoneinfo-parse/tree/master/data-crate-builder).
