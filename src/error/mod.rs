use std::result;
use std::io;
use std::fmt;
use bincode::serde;
use std::error::Error;

/// Subotai error type. It reports the various ways in which a hash table query may fail.
#[derive(Debug)]
pub enum SubotaiError {
   /// No response from a particular remote node.
   NoResponse,
   /// This node isn't connected to enough live nodes to be considered part of a live network,
   /// which caused this operation to fail (i.e. the node is off grid).
   OffGridError,
   /// The node specified wasn't found.
   NodeNotFound,
   /// The value specified falls out of bounds of the routing table space.
   OutOfBounds,
   /// Error during a store operation.
   StorageError,
   /// The network is unresponsive (several RPCs have timed out).
   UnresponsiveNetwork,
   Io(io::Error),
   Deserialize(serde::DeserializeError),
}

pub type SubotaiResult<T> = result::Result<T, SubotaiError>;

impl fmt::Display for SubotaiError {
   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
      match *self {
         SubotaiError::NoResponse => write!(f, "Timed out while waiting for node response."),
         SubotaiError::OffGridError => write!(f, "The node is currently off-grid."),
         SubotaiError::NodeNotFound => write!(f, "Could not find the node locally or in the network."),
         SubotaiError::OutOfBounds => write!(f, "Index falls out of routing table."),
         SubotaiError::StorageError => write!(f, "Corrupted Storage."),
         SubotaiError::UnresponsiveNetwork => write!(f, "Network too small or unresponsive."),
         SubotaiError::Io(ref err) => err.fmt(f),
         SubotaiError::Deserialize(ref err) => err.fmt(f),
      }
   }
}

impl Error for SubotaiError {
   fn description(&self) -> &str {
      match *self {
         SubotaiError::NoResponse => "Timed out with no response",
         SubotaiError::OffGridError => "The node is currently off-grid.",
         SubotaiError::NodeNotFound => "Could not find the node",
         SubotaiError::OutOfBounds => "Index outside routing table.",
         SubotaiError::StorageError => "Corrupted Storage.",
         SubotaiError::UnresponsiveNetwork => "Network too small or unresponsive.",
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
