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
   /// Sender node info (IP address updated on reception).
   pub sender     : routing::NodeInfo,
}

impl Rpc {
   /// Constructs a ping RPC. Pings simply carry information about the
   /// sender, and expect a response indicating that the receiving node
   /// is alive.
   pub fn ping(sender: routing::NodeInfo) -> Rpc {
      Rpc { kind: Kind::Ping, sender: sender }
   }

   /// Constructs a ping response. 
   pub fn ping_response(sender: routing::NodeInfo) -> Rpc {
      Rpc { kind: Kind::PingResponse, sender: sender }
   }

   /// Constructs an RPC asking for a the results of a table node lookup. The objective
   /// of this RPC is to locate a particular node while minimizing network traffic. In other
   /// words, the process short-circuits when the target node is found.
   pub fn locate(sender: routing::NodeInfo, id_to_find: SubotaiHash) -> Rpc {
      let payload = Arc::new(LocatePayload { id_to_find: id_to_find });
      Rpc { kind: Kind::Locate(payload), sender: sender }
   }

   /// Constructs an RPC with the response to a locate RPC.
   pub fn locate_response(sender: routing::NodeInfo, id_to_find: SubotaiHash, result: routing::LookupResult) -> Rpc {
      let payload = Arc::new(LocateResponsePayload { id_to_find: id_to_find, result: result} );
      Rpc { kind: Kind::LocateResponse(payload), sender: sender }
   }

   /// Constructs an RPC asking for a the results of a storage lookup.  
   pub fn retrieve(sender: routing::NodeInfo, key_to_find: SubotaiHash) -> Rpc {
      let payload = Arc::new(RetrievePayload { key_to_find: key_to_find });
      Rpc { kind: Kind::Retrieve(payload), sender: sender }
   }

   /// Constructs an RPC asking for a the results of a storage lookup.
   pub fn retrieve_response(sender: routing::NodeInfo, key_to_find: SubotaiHash, result: RetrieveResult) -> Rpc {
      let payload = Arc::new(RetrieveResponsePayload { key_to_find: key_to_find, result: result });
      Rpc { kind: Kind::RetrieveResponse(payload), sender: sender }
   }

   /// Constructs a probe RPC. It asks the receiving node to provide a list of
   /// K nodes close to a given node. It's a simpler version of the locate 
   /// RPC, that doesn't end early if the node is found.
   pub fn probe(sender: routing::NodeInfo, id_to_probe: SubotaiHash) -> Rpc {
      let payload = Arc::new(ProbePayload { id_to_probe: id_to_probe });
      Rpc { kind: Kind::Probe(payload), sender: sender }
   }

   /// Constructs the response to a probe RPC.
   pub fn probe_response(sender: routing::NodeInfo,
                         nodes: Vec<routing::NodeInfo>,
                         id_to_probe: SubotaiHash) -> Rpc {
      let payload = Arc::new(ProbeResponsePayload { id_to_probe: id_to_probe, nodes: nodes } );
      Rpc { kind: Kind::ProbeResponse(payload), sender: sender }
   }

   /// Constructs a store RPC. It asks the receiving node to store a key->value pair.
   pub fn store(sender: routing::NodeInfo, key: SubotaiHash, value: SubotaiHash) -> Rpc {
      let payload = Arc::new(StorePayload { key: key, value: value });     
      Rpc { kind: Kind::Store(payload), sender: sender }
   }

   /// Constructs a response to the store RPC, including the key and the operation result.
   pub fn store_response(sender: routing::NodeInfo, key: SubotaiHash, result: storage::StoreResult) -> Rpc {
      let payload = Arc::new(StoreResponsePayload { key: key, result: result });     
      Rpc { kind: Kind::StoreResponse(payload), sender: sender }
   }

   /// Serializes an RPC to be send over TCP. 
   pub fn serialize(&self) -> Vec<u8> {
       serde::serialize(&self, bincode::SizeLimit::Bounded(node::SOCKET_BUFFER_SIZE_BYTES as u64)).unwrap()
   }

   /// Deserializes into an RPC structure.
   pub fn deserialize(serialized: &[u8]) -> serde::DeserializeResult<Rpc> {
       serde::deserialize(serialized)
   }

   /// Reports whether the RPC is a LocateResponse looking
   /// for a particular node
   pub fn is_finding_node(&self, id: &SubotaiHash) -> bool {
      match self.kind {
         Kind::LocateResponse( ref payload ) => &payload.id_to_find == id,
         _ => false,
      }
   }

   /// Reports whether the RPC is a LocateResponse that found
   /// a particular node. If it was, returns the node.
   pub fn found_node(&self, id: &SubotaiHash) -> Option<routing::NodeInfo> {
      if let Kind::LocateResponse(ref payload) = self.kind {
         match payload.result {
            routing::LookupResult::Found(ref node) if &payload.id_to_find == id => return Some(node.clone()),
            _ => return None,
         }
      }
      None
   }

   /// Reports whether the RPC is a LocateResponse that failed to locate.
   /// If so, provides the closest nodes.
   pub fn did_not_find_node(&self, id: &SubotaiHash) -> Option<Vec<routing::NodeInfo>> {
      if let Kind::LocateResponse(ref payload) = self.kind {
         match payload.result {
            routing::LookupResult::ClosestNodes(ref nodes) if &payload.id_to_find == id => return Some(nodes.clone()),
            _ => return None,
         }
      }
      None
   }

   /// Reports whether the RPC is a RetrieveResponse looking
   /// for a particular key
   pub fn is_finding_value(&self, key: &SubotaiHash) -> bool {
      match self.kind {
         Kind::RetrieveResponse( ref payload ) => &payload.key_to_find == key,
         _ => false,
      }
   }

   /// Reports whether the RPC is a RetrieveResponse that found
   /// a particular key
   pub fn found_value(&self, key: &SubotaiHash) -> bool {
      if let Kind::RetrieveResponse(ref payload) = self.kind {
         match payload.result {
            RetrieveResult::Found(_) => return &payload.key_to_find == key,
            _ => return false,
         }
      }
      false
   }
}

/// Types of Subotai RPCs. Some of them contain reference counted payloads.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum Kind {
   Ping,
   PingResponse,
   Store(Arc<StorePayload>),
   StoreResponse(Arc<StoreResponsePayload>),
   Locate(Arc<LocatePayload>),
   LocateResponse(Arc<LocateResponsePayload>),
   Retrieve(Arc<RetrievePayload>),
   RetrieveResponse(Arc<RetrieveResponsePayload>),
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
pub struct LocatePayload {
   pub id_to_find    : SubotaiHash,
}

/// Includes the ID to find and the results of the table lookup.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct LocateResponsePayload {
   pub id_to_find : SubotaiHash,
   pub result     : routing::LookupResult,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum RetrieveResult {
   Found(SubotaiHash),
   Closest(Vec<routing::NodeInfo>),
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct RetrievePayload {
   pub key_to_find : SubotaiHash,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct RetrieveResponsePayload {
   pub key_to_find : SubotaiHash,
   pub result      : RetrieveResult,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct ProbePayload {
   pub id_to_probe : SubotaiHash,
}

/// Includes a vector of up to 'K' nodes close to the id to probe.
/// If the ID provided corresponded to the key in a key-value pair,
/// the corresponding value is also included in the response.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct ProbeResponsePayload {
   pub id_to_probe  : SubotaiHash,
   pub nodes        : Vec<routing::NodeInfo>,
}

#[cfg(test)]
mod tests {
   use super::*;
   use hash::SubotaiHash;
   use std::net;
   use std::str::FromStr;
   use routing;

   #[test]
   fn serdes_for_ping() {
      let ping = Rpc::ping(node_info_no_net(SubotaiHash::random()));
      let serialized_ping = ping.serialize();
      let deserialized_ping = Rpc::deserialize(&serialized_ping).unwrap();
      assert_eq!(ping, deserialized_ping);
   }

   fn node_info_no_net(id : SubotaiHash) -> routing::NodeInfo {
      routing::NodeInfo {
         id : id,
         address : net::SocketAddr::from_str("0.0.0.0:0").unwrap(),
      }
   }
}
