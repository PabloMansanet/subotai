use std::result;
use std::io;
use std::fmt;
use std::error::Error;

#[derive(Debug)]
pub enum SubotaiError {
   NoResponse,
   Io(io::Error),
}

pub type SubotaiResult<T> = result::Result<T, SubotaiError>;

impl fmt::Display for SubotaiError {
   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      match *self {
         SubotaiError::Io(ref err) => err.fmt(f),
         SubotaiError::NoResponse => write!(f, "Timed out while waiting for node response."),
      }
   }
}

impl Error for SubotaiError {
    fn description(&self) -> &str {
        match *self {
            SubotaiError::Io(ref err) => err.description(),
            SubotaiError::NoResponse => "Timed out with no response",
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            SubotaiError::Io(ref err) => Some(err),
            SubotaiError::NoResponse => None,
        }
    }
}

impl From<io::Error> for SubotaiError {
    fn from(err: io::Error) -> SubotaiError {
        SubotaiError::Io(err)
    }
}
