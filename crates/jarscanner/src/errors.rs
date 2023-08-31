use serde_json;
use std::error::Error;
use std::fmt;
use std::io;
use zip::result::ZipError;

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

#[derive(Debug)]
pub enum JarscannerError {
    IoError(io::Error),
    ZipError(ZipError),
    SerdeError(serde_json::Error),
    LabelToAllowedPrefixesError(LabelToAllowedPrefixesError),
}

impl fmt::Display for JarscannerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            JarscannerError::IoError(e) => write!(f, "IO error: {}", e),
            JarscannerError::ZipError(e) => write!(f, "ZIP error: {}", e),
            JarscannerError::SerdeError(e) => write!(f, "Serialization error: {}", e),
            JarscannerError::LabelToAllowedPrefixesError(e) => {
                write!(f, "Label to allowed prefixes error: {}", e)
            }
        }
    }
}

impl Error for JarscannerError {}

impl From<io::Error> for JarscannerError {
    fn from(err: io::Error) -> JarscannerError {
        JarscannerError::IoError(err)
    }
}

impl From<ZipError> for JarscannerError {
    fn from(err: ZipError) -> JarscannerError {
        JarscannerError::ZipError(err)
    }
}

impl From<serde_json::Error> for JarscannerError {
    fn from(err: serde_json::Error) -> JarscannerError {
        JarscannerError::SerdeError(err)
    }
}

impl From<LabelToAllowedPrefixesError> for JarscannerError {
    fn from(err: LabelToAllowedPrefixesError) -> JarscannerError {
        JarscannerError::LabelToAllowedPrefixesError(err)
    }
}
