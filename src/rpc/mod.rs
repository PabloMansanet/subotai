use std::boxed::Box;
use hash;
use rpc;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct Rpc {
   destination : Destination,
   payload     : Payload,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub enum Destination {
   Address(String),
   Id(hash::Hash),
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub enum Payload {
   Ping,
   PingResponse,
   Store(StorePayload),
   FindNode(FindNodePayload),
   FindNodeResponse(FindNodeResponsePayload),
   FindValue(FindValuePayload),
   FindValueResponse(FindValueResponsePayload),
}

impl Rpc {
   pub fn ping(destination: Destination) -> Rpc {
      Rpc {
         destination : destination,
         payload     : Payload::Ping,
      }
   }
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
    use bincode::serde::{serialize, deserialize};
    use bincode::SizeLimit;
    use node::SOCKET_BUFFER_SIZE_BYTES;

    #[test]
    fn serdes_for_ping() {
       let destination = Destination::Id(Hash::random());
       let ping = Rpc::ping(destination);
       let serialized_ping = serialize(&ping, SizeLimit::Bounded(SOCKET_BUFFER_SIZE_BYTES as u64)).unwrap();
       let deserialized_ping: Rpc = deserialize(&serialized_ping).unwrap();

       assert_eq!(ping, deserialized_ping);
    }
}
