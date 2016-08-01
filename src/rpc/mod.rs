use bincode::serde;
use routing;
use bincode;
use node;
use std::sync::Arc;
use hash::SubotaiHash;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct Rpc {
   pub kind        : Kind,
   pub sender_id   : SubotaiHash,
   pub reply_port  : u16,
}

impl Rpc {
   pub fn ping(sender_id: SubotaiHash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::Ping, sender_id: sender_id, reply_port: reply_port }
   }

   pub fn ping_response(sender_id: SubotaiHash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::PingResponse, sender_id: sender_id, reply_port: reply_port }
   }

   /// Asks for a the results of a table node lookup.
   pub fn find_node(sender_id: SubotaiHash, reply_port: u16, id_to_find: SubotaiHash, nodes_wanted: usize) -> Rpc {
      let payload = Arc::new(FindNodePayload { id_to_find: id_to_find, nodes_wanted: nodes_wanted });
      Rpc { kind: Kind::FindNode(payload), sender_id: sender_id, reply_port: reply_port }
   }

   /// Encapsulates the response to a previously requested find_node RPC.
   pub fn find_node_response(sender_id: SubotaiHash, reply_port: u16, id_to_find: SubotaiHash, result: routing::LookupResult) -> Rpc {
      let payload = Arc::new(FindNodeResponsePayload { id_to_find: id_to_find, result: result} );
      Rpc { kind: Kind::FindNodeResponse(payload), sender_id: sender_id, reply_port: reply_port }
   }

   pub fn bootstrap(sender_id: SubotaiHash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::Bootstrap, sender_id: sender_id, reply_port: reply_port }
   }

   pub fn bootstrap_response(sender_id: SubotaiHash, reply_port: u16, nodes: Vec<routing::NodeInfo>) -> Rpc {
      let payload = Arc::new(BootstrapResponsePayload { nodes: nodes } );
      Rpc { kind: Kind::BootstrapResponse(payload), sender_id: sender_id, reply_port: reply_port }
   }

   pub fn serialize(&self) -> Vec<u8> {
       serde::serialize(&self, bincode::SizeLimit::Bounded(node::SOCKET_BUFFER_SIZE_BYTES as u64)).unwrap()
   }

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
            routing::LookupResult::Myself | routing::LookupResult::Found(_) => return &payload.id_to_find == id,
            _ => return false,
         }
      }
      false
   }
}

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

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindNodePayload {
   pub id_to_find    : SubotaiHash,
   pub nodes_wanted  : usize,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindValuePayload;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindNodeResponsePayload {
   pub id_to_find : SubotaiHash,
   pub result     : routing::LookupResult,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct BootstrapResponsePayload {
   pub nodes: Vec<routing::NodeInfo>,
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
