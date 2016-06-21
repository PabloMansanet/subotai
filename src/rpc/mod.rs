use bincode::serde;
use bincode;
use node;
use hash;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct Rpc {
   pub kind        : Kind,
   pub sender_id   : hash::Hash,
   pub reply_port  : u16,
}

impl Rpc {
   pub fn ping(sender_id: hash::Hash, reply_port: u16) -> Rpc {
      Rpc { kind: Kind::Ping, sender_id: sender_id, reply_port: reply_port }
   }

   pub fn serialize(&self) -> Vec<u8> {
       serde::serialize(&self, bincode::SizeLimit::Bounded(node::SOCKET_BUFFER_SIZE_BYTES as u64)).unwrap()
   }

   pub fn deserialize(serialized: &[u8]) -> serde::DeserializeResult<Rpc> {
       serde::deserialize(serialized)
   }
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub enum Kind {
   Ping,
   PingResponse,
   Store(StorePayload),
   FindNode(FindNodePayload),
   FindNodeResponse(FindNodeResponsePayload),
   FindValue(FindValuePayload),
   FindValueResponse(FindValueResponsePayload),
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct StorePayload;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct FindNodePayload;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct FindValuePayload;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct FindNodeResponsePayload;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
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
