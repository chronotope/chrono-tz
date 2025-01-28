fn main() {
    #[cfg(feature = "filter-by-regex")]
    chrono_tz_build::main();

    #[cfg(not(feature = "filter-by-regex"))]
    {
        if std::env::var("CHRONO_TZ_TIMEZONE_FILTER").is_ok() {
            println!("cargo:warning=CHRONO_TZ_TIMEZONE_FILTER set without enabling filter-by-regex feature")
        }
    }
}
