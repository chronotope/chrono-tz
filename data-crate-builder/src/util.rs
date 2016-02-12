//! Helpers and stuff.

/// Helper macro that mimics `println!`, only it writes to stderr instead.
///
/// Stolen from http://stackoverflow.com/a/27590832
#[macro_export]
macro_rules! println_stderr {
    ($($arg:tt)*) => (
        if let Err(x) = writeln!(&mut stderr(), $($arg)* ) {
            panic!("Unable to write to stderr: {}", x);
        }
    )
}
