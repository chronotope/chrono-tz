#[macro_export]
macro_rules! println_stderr {
    ($($arg:tt)*) => (
        if let Err(x) = writeln!(&mut stderr(), $($arg)* ) {
            panic!("Unable to write to stderr: {}", x);
        }
    )
}
