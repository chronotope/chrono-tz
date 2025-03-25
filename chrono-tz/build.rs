fn main() {
    println!("cargo:rerun-if-env-changed=CHRONO_TZ_TIMEZONE_FILTER");

    #[cfg(feature = "filter-by-regex")]
    chrono_tz_build::main();

    #[cfg(not(feature = "filter-by-regex"))]
    {
        if std::env::var("CHRONO_TZ_TIMEZONE_FILTER").is_ok() {
            println!("cargo::error=CHRONO_TZ_TIMEZONE_FILTER set without enabling filter-by-regex feature")
        }
    }
}
