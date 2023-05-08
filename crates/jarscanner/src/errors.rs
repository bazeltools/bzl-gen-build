use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct FileNameError {
    message: String,
}

impl FileNameError {
    pub fn new(msg: String) -> FileNameError {
        FileNameError { message: msg }
    }
}

impl fmt::Display for FileNameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for FileNameError {
    fn description(&self) -> &str {
        &self.message
    }
}
