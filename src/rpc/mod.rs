use std::boxed::Box;
use hash;
use rpc;

#[derive(RustcDecodable, RustcEncodable)]
pub struct Rpc {
   destination : Destination,
   payload     : Payload,
}

#[derive(RustcDecodable, RustcEncodable)]
pub enum Destination {
   Address(String),
   Id(hash::Hash),
}

#[derive(RustcDecodable, RustcEncodable)]
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

#[derive(RustcDecodable, RustcEncodable)]
pub struct StorePayload;

#[derive(RustcDecodable, RustcEncodable)]
pub struct FindNodePayload;

#[derive(RustcDecodable, RustcEncodable)]
pub struct FindValuePayload;

#[derive(RustcDecodable, RustcEncodable)]
pub struct FindNodeResponsePayload;

#[derive(RustcDecodable, RustcEncodable)]
pub struct FindValueResponsePayload;
