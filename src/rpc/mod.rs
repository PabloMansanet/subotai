//! #Remote Procedure Call. 
//!
//! Subotai RPCs are the packets sent over TCP between nodes. They
//! contain information about the sender, as well as an optional payload.

use bincode::serde;
use {routing, bincode, node, storage};
use std::sync::Arc;
use hash::SubotaiHash;

/// Serializable struct implementation of an RPC.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct Rpc {
   /// Category of RPC.
   pub kind       : Kind,
   /// Hash ID of the original sender.
   pub sender_id  : SubotaiHash,
   /// Port through which the original sender is listening for responses.
   pub reply_port : u16,
}

impl Rpc {
   /// Constructs a ping RPC. Pings simply carry information about the
   /// sender, and expect a response indicating that the receiving node
   /// is alive.
   pub fn ping(sender_id: SubotaiHash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::Ping, sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs a ping response. 
   pub fn ping_response(sender_id: SubotaiHash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::PingResponse, sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs an RPC asking for a the results of a table node lookup.
   pub fn find_node(sender_id: SubotaiHash, reply_port: u16, id_to_find: SubotaiHash, nodes_wanted: usize) -> Rpc {
      let payload = Arc::new(FindNodePayload { id_to_find: id_to_find, nodes_wanted: nodes_wanted });
      Rpc { kind: Kind::FindNode(payload), sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs an RPC with the response to a find_node RPC.
   pub fn find_node_response(sender_id: SubotaiHash, reply_port: u16, id_to_find: SubotaiHash, result: routing::LookupResult) -> Rpc {
      let payload = Arc::new(FindNodeResponsePayload { id_to_find: id_to_find, result: result} );
      Rpc { kind: Kind::FindNodeResponse(payload), sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs a probe RPC. It asks the receiving node to provide a list of
   /// K nodes close to a given node. It's a simpler version of the find_node 
   /// RPC, that doesn't end early if the node is found.
   pub fn probe(sender_id: SubotaiHash, reply_port: u16, id_to_probe: SubotaiHash) -> Rpc {
      let payload = Arc::new(ProbePayload { id_to_probe: id_to_probe });
      Rpc { kind: Kind::Probe(payload), sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs the response to a probe RPC.
   pub fn probe_response(sender_id: SubotaiHash, 
                         reply_port: u16, 
                         nodes: Vec<routing::NodeInfo>,
                         id_to_probe: SubotaiHash) -> Rpc {
      let payload = Arc::new(ProbeResponsePayload { id_to_probe: id_to_probe, nodes: nodes } );
      Rpc { kind: Kind::ProbeResponse(payload), sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs a store RPC. It asks the receiving node to store a key->value pair.
   pub fn store(sender_id: SubotaiHash, reply_port: u16, key: SubotaiHash, value: SubotaiHash) -> Rpc {
      let payload = Arc::new(StorePayload { key: key, value: value });     
      Rpc { kind: Kind::Store(payload), sender_id: sender_id, reply_port: reply_port }
   }

   /// Constructs a response to the store RPC, including the key and the operation result.
   pub fn store_response(sender_id: SubotaiHash, reply_port: u16, key: SubotaiHash, result: storage::StoreResult) -> Rpc {
      let payload = Arc::new(StoreResponsePayload { key: key, result: result });     
      Rpc { kind: Kind::StoreResponse(payload), sender_id: sender_id, reply_port: reply_port }
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
   /// for a particular node
   pub fn is_finding_node(&self, id: &SubotaiHash) -> bool {
      match self.kind {
         Kind::FindNodeResponse( ref payload ) => &payload.id_to_find == id,
         _ => false,
      }
   }

   /// Reports whether the RPC is a FindNodeResponse that found
   /// a particular node
   pub fn found_node(&self, id: &SubotaiHash) -> bool {
      if let Kind::FindNodeResponse(ref payload) = self.kind {
         match payload.result {
            routing::LookupResult::Found(_) => return &payload.id_to_find == id,
            _ => return false,
         }
      }
      false
   }
}

/// Types of RPC contemplated by the standard, plus some unique to the Subotai
/// DHT (such as as a specialized Probe RPC). Some include reference
/// counted payloads.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum Kind {
   Ping,
   PingResponse,
   Store(Arc<StorePayload>),
   StoreResponse(Arc<StoreResponsePayload>),
   FindNode(Arc<FindNodePayload>),
   FindNodeResponse(Arc<FindNodeResponsePayload>),
   FindValue(Arc<FindValuePayload>),
   FindValueResponse(Arc<FindValueResponsePayload>),
   Probe(Arc<ProbePayload>),
   ProbeResponse(Arc<ProbeResponsePayload>)
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct StorePayload {
   pub key   : SubotaiHash,
   pub value : SubotaiHash,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct StoreResponsePayload {
   pub key    : SubotaiHash,
   pub result : storage::StoreResult,
}

/// Includes the ID to find and the amount of nodes required.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindNodePayload {
   pub id_to_find    : SubotaiHash,
   pub nodes_wanted  : usize,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindValuePayload;

/// Includes the ID to find and the results of the table lookup.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindNodeResponsePayload {
   pub id_to_find : SubotaiHash,
   pub result     : routing::LookupResult,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct ProbePayload {
   pub id_to_probe : SubotaiHash,
}

/// Includes a vector of up to 'K' nodes close to the id to probe.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct ProbeResponsePayload {
   pub id_to_probe : SubotaiHash,
   pub nodes       : Vec<routing::NodeInfo>,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindValueResponsePayload;

#[cfg(test)]
mod tests {
    use super::*;
    use hash::SubotaiHash;

    #[test]
    fn serdes_for_ping() {
       let ping = Rpc::ping(SubotaiHash::random(), 50000);
       let serialized_ping = ping.serialize();
       let deserialized_ping = Rpc::deserialize(&serialized_ping).unwrap();
       assert_eq!(ping, deserialized_ping);
    }
}
