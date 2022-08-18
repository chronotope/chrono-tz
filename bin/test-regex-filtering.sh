#!/usr/bin/env bash

set -euxo pipefail

export RUST_BACKTRACE=1
export CHRONO_TZ_TIMEZONE_FILTER='(Europe/London|GMT|UTC)'

cd check-regex-filtering

cargo test --color=always -- --color=always
