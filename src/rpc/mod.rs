use std::boxed::Box;
use bincode::serde;
use bincode;
use node;
use hash;
use rpc;

#[derive(Eq, PartialEq, Debug)]
pub struct Rpc {
   destination : Destination,
   payload     : Payload,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub enum Destination {
   Address(String),
   Id(hash::Hash),
   Unknown,
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

   pub fn serialized_payload(&self) -> Vec<u8> {
       serde::serialize(&self.payload, bincode::SizeLimit::Bounded(node::SOCKET_BUFFER_SIZE_BYTES as u64)).unwrap()
   }

   pub fn from_payload(serialized_payload: &[u8]) -> serde::DeserializeResult<Rpc> {
       let payload: Payload = try!(serde::deserialize(serialized_payload));
       Ok(Rpc { 
          destination : Destination::Unknown,
          payload : payload,
       })
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

    #[test]
    fn serdes_for_ping() {
       let destination = Destination::Id(Hash::random());
       let ping = Rpc::ping(destination);
       let serialized_payload = ping.serialized_payload();
       let deserialized_ping = Rpc::from_payload(&serialized_payload).unwrap();

       assert_eq!(ping.payload, deserialized_ping.payload);
       assert_eq!(Destination::Unknown, deserialized_ping.destination);
    }
}
