Chrono-tz Changelog
===================

## 0.8.3

* **tzdb** Update tzdb from 2023b to 2023c. All changes in 2023b have been
    reverted back to 2023a. The full list of changes can be found
    [here](https://mm.icann.org/pipermail/tz-announce/2023-March/000079.html)

## 0.8.2

* **tzdb** Update tzdb from 2022f to 2023b. Some timezones have been linked. For
  the full list, check the
  [2023a](https://mm.icann.org/pipermail/tz-announce/2023-March/000077.html) and
  [2023b](https://mm.icann.org/pipermail/tz-announce/2023-March/000078.html) announcements.

## 0.8.1

* **tzdb** Update tzdb from 2022f to 2022g.

## 0.8.0

* **tzdb** Update tzdb from 2022e to 2022f. Some timezones have been removed. For
  the full list, check
  [here](https://mm.icann.org/pipermail/tz-announce/2022-October/000075.html).

## 0.7

* **tzdb** Update tzdb from 2022a to 2022e, some timezones have been removed for
    the full list check
    [here](https://mm.icann.org/pipermail/tz-announce/2022-August/000071.html).

## 0.6.2

* **tzdb** Update tzdb to 2022a.

* Bump the [`phf`](https://github.com/rust-phf/rust-phf) family of dependencies
  to v0.11.

## 0.6.1

* **tzdb** Update tzdb to 2021e.

## 0.6.0

* **tzdb** [breaking change] Update tzdb to 2020b, which removes the `US/Pacific-New` timezone.
  https://github.com/eggert/tz/commit/284e877d7511d964249ecfd2e75a9cab85e2741a

* **feature** Add support for filtering the set of timezones with a new `filter-by-regex` feature
  which uses `CHRONO_TZ_TIMEZONE_FILTER` env var. It should be set to a regular expression of
  timezones to include.

* **feature** Add support for case-insensitive timezone matching via the `case-insensitive`
  feature.
