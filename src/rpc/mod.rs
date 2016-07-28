//! #Remote Procedure Call. 
//!
//! Subotai RPCs are the packets sent over TCP between nodes. They
//! contain information about the sender, as well as an optional payload.

use bincode::serde;
use {routing, bincode, node};
use std::sync::Arc;
use hash::Hash;

/// Serializable struct implementation of an RPC.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct Rpc {
   pub kind        : Kind,
   /// Hash ID of the original sender.
   pub sender_id   : Hash,
   /// Port through which the original sender is listening for responses.
   pub reply_port  : u16,
}

impl Rpc {
   /// Constructs a ping RPC. Pings simply carry information about the
   /// sender, and expect a response indicating that the receiving node
   /// is alive.
   pub fn ping(sender_id: Hash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::Ping, sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs a ping response. 
   pub fn ping_response(sender_id: Hash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::PingResponse, sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs an RPC asking for a the results of a table node lookup.
   pub fn find_node(sender_id: Hash, reply_port: u16, id_to_find: Hash, nodes_wanted: usize) -> Rpc {
      let payload = Arc::new(FindNodePayload { id_to_find: id_to_find, nodes_wanted: nodes_wanted });
      Rpc { kind: Kind::FindNode(payload), sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs an RPC with the response to a find_node RPC.
   pub fn find_node_response(sender_id: Hash, reply_port: u16, id_to_find: Hash, result: routing::LookupResult) -> Rpc {
      let payload = Arc::new(FindNodeResponsePayload { id_to_find: id_to_find, result: result} );
      Rpc { kind: Kind::FindNodeResponse(payload), sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs a bootstrap RPC. It asks the receiving node to provide a list of
   /// nodes close to the sender, to facilitate joining the network.
   pub fn bootstrap(sender_id: Hash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::Bootstrap, sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs the response to a bootstrap RPC.
   pub fn bootstrap_response(sender_id: Hash, reply_port: u16, nodes: Vec<routing::NodeInfo>) -> Rpc {
      let payload = Arc::new(BootstrapResponsePayload { nodes: nodes } );
      Rpc { kind: Kind::BootstrapResponse(payload), sender_id: sender_id, reply_port: reply_port }
   }

   /// Serializes an RPC to be send over TCP. 
   pub fn serialize(&self) -> Vec<u8> {
       serde::serialize(&self, bincode::SizeLimit::Bounded(node::SOCKET_BUFFER_SIZE_BYTES as u64)).unwrap()
   }

   /// Deserializes into an RPC structure.
   pub fn deserialize(serialized: &[u8]) -> serde::DeserializeResult<Rpc> {
       serde::deserialize(serialized)
   }

   /// Reports whether the RPC is a FindNodeResponse looking
   /// for a particular node.
   pub fn is_finding_node(&self, id: &Hash) -> bool {
      match self.kind {
         Kind::FindNodeResponse( ref payload ) => &payload.id_to_find == id,
         _ => false,
      }
   }

   /// Reports whether the RPC is a FindNodeResponse that found
   /// a particular node.
   pub fn found_node(&self, id: &Hash) -> bool {
      if let Kind::FindNodeResponse(ref payload) = self.kind {
         match payload.result {
            routing::LookupResult::Myself | routing::LookupResult::Found(_) => return &payload.id_to_find == id,
            _ => return false,
         }
      }
      false
   }
}

/// Types of RPC contemplated by the standard, plus some unique to the Subotai
/// DHT (such as as a specialized Bootstrap RPC). Some include reference
/// counted payloads.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum Kind {
   Ping,
   PingResponse,
   Store(Arc<StorePayload>),
   FindNode(Arc<FindNodePayload>),
   FindNodeResponse(Arc<FindNodeResponsePayload>),
   FindValue(Arc<FindValuePayload>),
   FindValueResponse(Arc<FindValueResponsePayload>),
   Bootstrap,
   BootstrapResponse(Arc<BootstrapResponsePayload>)
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct StorePayload;

/// Includes the ID to find and the amount of nodes required.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindNodePayload {
   pub id_to_find    : Hash,
   pub nodes_wanted  : usize,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindValuePayload;

/// Includes the ID to find and the results of the table lookup.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindNodeResponsePayload {
   pub id_to_find : Hash,
   pub result     : routing::LookupResult,
}

/// Includes a vector of up to 'K' nodes close to the respondee.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct BootstrapResponsePayload {
   pub nodes: Vec<routing::NodeInfo>,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindValueResponsePayload;

#[cfg(test)]
mod tests {
    use super::*;
    use hash::Hash;

    #[test]
    fn serdes_for_ping() {
       let ping = Rpc::ping(Hash::random(), 50000);
       let serialized_ping = ping.serialize();
       let deserialized_ping = Rpc::deserialize(&serialized_ping).unwrap();
       assert_eq!(ping, deserialized_ping);
    }
}
