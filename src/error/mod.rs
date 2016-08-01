use std::result;
use std::io;
use std::fmt;
use bincode::serde;
use std::error::Error;

#[derive(Debug)]
pub enum SubotaiError {
   NoResponse,
   NodeNotFound,
   StorageFull,
   UnresponsiveNetwork,
   Io(io::Error),
   Deserialize(serde::DeserializeError),
}

pub type SubotaiResult<T> = result::Result<T, SubotaiError>;

impl fmt::Display for SubotaiError {
   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      match *self {
         SubotaiError::NoResponse => write!(f, "Timed out while waiting for node response."),
         SubotaiError::NodeNotFound => write!(f, "Could not find the node locally or in the network."),
         SubotaiError::UnresponsiveNetwork => write!(f, "Network too small or unresponsive."),
         SubotaiError::StorageFull => write!(f, "The value storage area for this node is full."),
         SubotaiError::Io(ref err) => err.fmt(f),
         SubotaiError::Deserialize(ref err) => err.fmt(f),
      }
   }
}

impl Error for SubotaiError {
   fn description(&self) -> &str {
      match *self {
         SubotaiError::NoResponse => "Timed out with no response",
         SubotaiError::NodeNotFound => "Could not find the node",
         SubotaiError::UnresponsiveNetwork => "Network too small or unresponsive.",
         SubotaiError::StorageFull => "Storage is full",
         SubotaiError::Io(ref err) => err.description(),
         SubotaiError::Deserialize(ref err) => err.description(),
      }
   }

   fn cause(&self) -> Option<&Error> {
      match *self {
         SubotaiError::Io(ref err) => Some(err),
         SubotaiError::Deserialize(ref err) => Some(err),
         _ => None,
      }
   }
}

impl From<io::Error> for SubotaiError {
   fn from(err: io::Error) -> SubotaiError {
      SubotaiError::Io(err)
   }
}

impl From<serde::DeserializeError> for SubotaiError {
   fn from(err: serde::DeserializeError) -> SubotaiError {
      SubotaiError::Deserialize(err)
   }
}
