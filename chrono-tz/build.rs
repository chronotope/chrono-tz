#[cfg(feature = "filter-by-regex")]
use chrono_tz_build::FILTER_ENV_VAR_NAME;

fn main() {
    #[cfg(feature = "filter-by-regex")]
    println!("cargo:rerun-if-env-changed={FILTER_ENV_VAR_NAME}");
    chrono_tz_build::main();
}
