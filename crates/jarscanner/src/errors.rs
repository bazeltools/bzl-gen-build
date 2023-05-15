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

pub struct LabelToAllowedPrefixesError {
    pub json_deser_error: String,
}

impl LabelToAllowedPrefixesError {
    fn write_error_msg(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Error parsing the JSON for --label-to-allowed-prefixes argument: {}",
            self.json_deser_error
        )
    }
}

impl fmt::Display for LabelToAllowedPrefixesError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.write_error_msg(f)
    }
}

// The Result that gets returned from the `main` entrypoint is going to use Debug, not Display, so we want to override this
impl fmt::Debug for LabelToAllowedPrefixesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_error_msg(f)
    }
}

impl Error for LabelToAllowedPrefixesError {}
