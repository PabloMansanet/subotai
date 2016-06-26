use bincode::serde;
use routing;
use bincode;
use node;
use hash::Hash;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct Rpc {
   pub kind        : Kind,
   pub sender_id   : Hash,
   pub reply_port  : u16,
}

impl Rpc {
   pub fn ping(sender_id: Hash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::Ping, sender_id: sender_id, reply_port: reply_port }
   }

   pub fn ping_response(sender_id: Hash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::PingResponse, sender_id: sender_id, reply_port: reply_port }
   }

   /// Asks for a the results of a table node lookup.
   pub fn find_node(sender_id: Hash, reply_port: u16, id_to_find: Hash, nodes_wanted: usize) -> Rpc {
      let payload = Box::new(FindNodePayload { id_to_find: id_to_find, nodes_wanted: nodes_wanted });
      Rpc { kind: Kind::FindNode(payload), sender_id: sender_id, reply_port: reply_port }
   }

   /// Encapsulates the response to a previously requested find_node RPC.
   pub fn find_node_response(sender_id: Hash, reply_port: u16, id_to_find: Hash, result: routing::LookupResult) -> Rpc {
      let payload = Box::new(FindNodeResponsePayload { id_to_find: id_to_find, result: result} );
      Rpc { kind: Kind::FindNodeResponse(payload), sender_id: sender_id, reply_port: reply_port }
   }

   pub fn serialize(&self) -> Vec<u8> {
       serde::serialize(&self, bincode::SizeLimit::Bounded(node::SOCKET_BUFFER_SIZE_BYTES as u64)).unwrap()
   }

   pub fn deserialize(serialized: &[u8]) -> serde::DeserializeResult<Rpc> {
       serde::deserialize(serialized)
   }
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub enum Kind {
   Ping,
   PingResponse,
   Store(Box<StorePayload>),
   FindNode(Box<FindNodePayload>),
   FindNodeResponse(Box<FindNodeResponsePayload>),
   FindValue(Box<FindValuePayload>),
   FindValueResponse(Box<FindValueResponsePayload>),
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct StorePayload;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindNodePayload {
   id_to_find    : Hash,
   nodes_wanted  : usize,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindValuePayload;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct FindNodeResponsePayload {
   id_to_find : Hash,
   result     : routing::LookupResult,
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
