//! Any errors that can happen ever.

use std::fmt;
use std::io::Error as IOError;

use getopts;

quick_error! {

    /// Anything that can go wrong at any stage in the program, causing it to
    /// return 1 instead of 0.
    #[derive(Debug)]
    pub enum Error {

        /// A file or directory couldn’t be read or written to.
        IO(err: IOError) {
            from()
            display(x) -> ("IO error: {}", err)
        }

        /// The `zoneinfo-parse` crate didn’t like one or more lines of input.
        Errors(errs: Errors) {
            from(es: Vec<ParseError>) -> (Errors(es))
            display(x) -> ("{}", errs)
        }

        /// The `getopts` crate didn’t like the user’s command-line args.
        Getopts(err: getopts::Fail) {
            from()
            display(x) -> ("Error parsing options: {}", err)
        }
    }
}


/// An error when the data crate builder couldn’t parse a line of input.
#[derive(Debug)]
pub struct ParseError {

    /// The filename that contained the line.
    pub filename: String,

    /// The number of the line that failed to be parsed.
    pub line: usize,

    /// A human-readable description of what the error was.
    pub error: String,
}


/// Wrapper around a vector of parse errors for a custom `fmt::Display`
/// implementation used by the definition in `quick-error!` above.
#[derive(Debug)]
pub struct Errors(Vec<ParseError>);

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        for err in &self.0 {
            write!(f, "{}:{}: {}\n", err.filename, err.line, err.error)?;
        }

        Ok(())
    }
}
