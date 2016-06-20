use std::net;
use hash;

pub enum Rpc {
   Ping(Box::<PingData>),
   Store(Box::<StoreData>),
   FindNode(Box::<FindNodeData>),
   FindValue(Box::<FindValueData>),
}

pub enum Destination {
   Address((net::IpAddr, u16)),
   id(hash::Hash),
}

pub struct RpcBuilder {
   destination: rpc::Destination,
}

impl RpcBuilder {
   fn destination(self, destination: rpc::Destination) -> Self {
      self.destination = destination;
      self
   }

   fn ping() -> Rpc {
      
   }
}

pub struct PingData {
   destination: Destination,
}
