#![no_std]
#![no_main]

use core::panic::PanicInfo;

use chrono_tz::US::Pacific;
use cortex_m_rt::entry;

#[panic_handler]
fn catch_panic(_e: &PanicInfo) -> ! {
    loop {}
}

#[entry]
fn main() -> ! {
    assert!(Pacific == Pacific);

    loop {}
}
