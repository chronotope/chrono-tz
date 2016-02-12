use std::fmt;
use std::io::Error as IOError;

use getopts;


quick_error! {
    #[derive(Debug)]
    pub enum Error {
        IO(err: IOError) {
            from()
            display(x) -> ("IO error: {}", err)
        }

        Errors(errs: Errors) {
            from(es: Vec<ParseError>) -> (Errors(es))
            display(x) -> ("{}", errs)
        }

        Getopts(err: getopts::Fail) {
            from()
            display(x) -> ("Error parsing options: {}", err)
        }
    }
}


#[derive(Debug)]
pub struct ParseError {
    pub filename: String,
    pub line: usize,
    pub error: String,
}

#[derive(Debug)]
pub struct Errors(Vec<ParseError>);

impl fmt::Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        for err in &self.0 {
            try!(write!(f, "{}:{}: {}\n", err.filename, err.line, err.error));
        }
        Ok(())
    }
}