use bincode::serde;
use bincode;
use node;
use hash;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct Rpc {
   kind: Kind,
}

impl Rpc {
   pub fn ping() -> Rpc {
      Rpc { kind: Kind::Ping }
   }

   pub fn serialize(&self) -> Vec<u8> {
       serde::serialize(&self, bincode::SizeLimit::Bounded(node::SOCKET_BUFFER_SIZE_BYTES as u64)).unwrap()
   }

   pub fn deserialize(serialized: &[u8]) -> serde::DeserializeResult<Rpc> {
       serde::deserialize(serialized)
   }
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
enum Kind {
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
       let ping = Rpc::ping();
       let serialized_ping = ping.serialize();
       let deserialized_ping = Rpc::deserialize(&serialized_ping).unwrap();
       assert_eq!(ping, deserialized_ping);
    }
}
