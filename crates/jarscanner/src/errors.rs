use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub struct FileNameError {
    message: String,
}

impl FileNameError {
    fn new(msg: &str) -> FileNameError {
        FileNameError {
            message: msg.to_string(),
        }
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
