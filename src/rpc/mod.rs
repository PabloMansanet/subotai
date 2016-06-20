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
   Ping(PingPayload),
   Store(StorePayload),
   FindNode(FindNodePayload),
   FindValue(FindValuePayload),
}

pub struct Builder {
   destination: rpc::Destination,
}

pub enum BuildError {
   InvalidConfiguration,
}

impl Builder {
   pub fn destination(mut self, destination: rpc::Destination) -> Self {
      self.destination = destination;
      self
   }

   pub fn ping(mut self) -> Result<rpc::Rpc, BuildError> {
      Ok(rpc::Rpc {
         destination : self.destination,
         payload     : Payload::Ping(PingPayload),
      })
   }
}

#[derive(RustcDecodable, RustcEncodable)]
pub struct PingPayload;

#[derive(RustcDecodable, RustcEncodable)]
pub struct StorePayload;

#[derive(RustcDecodable, RustcEncodable)]
pub struct FindNodePayload;

#[derive(RustcDecodable, RustcEncodable)]
pub struct FindValuePayload;



