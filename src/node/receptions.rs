use bus;
use hash::Hash;
use rpc;
use time;
use node::resources;

/// A blocking iterator over the RPCs received by a node.
pub struct Receptions {
   iter          : bus::BusIntoIter<resources::Update>,
   timeout       : Option<time::SteadyTime>,
   rpc_filter    : Option<RpcFilter>,
   sender_filter : Option<Vec<Hash>>,
}

#[derive(Eq, PartialEq, Debug)]
pub enum RpcFilter {
   Ping,
   PingResponse,
   Store,
   FindNode,
   FindNodeResponse,
   FindValue,
   FindValueResponse,
}

impl Receptions {
   pub fn new(resources: &resources::Resources) -> Receptions {
      Receptions {
         iter          : resources.updates.lock().unwrap().add_rx().into_iter(),
         timeout       : None,
         rpc_filter    : None,
         sender_filter : None
      }
   }

   /// Restricts the iterator to a particular span of time.
   pub fn during(mut self, lifespan: time::Duration) -> Receptions {
      self.timeout = Some(time::SteadyTime::now() + lifespan);
      self
   }

   /// Only produces a particular rpc
   pub fn rpc(mut self, filter: RpcFilter) -> Receptions {
      self.rpc_filter = Some(filter);
      self
   }

   /// Only from a set of senders
   pub fn from(mut self, senders: Vec<Hash>) -> Receptions {
      self.sender_filter = Some(senders);
      self
   }
}

impl Iterator for Receptions {
   type Item = rpc::Rpc;

   fn next(&mut self) -> Option<rpc::Rpc> {
      loop {
         if let Some(timeout) = self.timeout {
            if time::SteadyTime::now() > timeout {
               break;
            }
         }

         if let Some(resources::Update::RpcReceived(rpc)) = self.iter.next() {
            if let Some(ref rpc_filter) = self.rpc_filter {
               match rpc.kind {
                  rpc::Kind::Ping                 => if *rpc_filter != RpcFilter::Ping { continue; },
                  rpc::Kind::PingResponse         => if *rpc_filter != RpcFilter::PingResponse { continue; },
                  rpc::Kind::Store(_)             => if *rpc_filter != RpcFilter::Store { continue; },
                  rpc::Kind::FindNode(_)          => if *rpc_filter != RpcFilter::FindNode { continue; },
                  rpc::Kind::FindNodeResponse(_)  => if *rpc_filter != RpcFilter::FindNodeResponse { continue; },
                  rpc::Kind::FindValue(_)         => if *rpc_filter != RpcFilter::FindValue { continue; },
                  rpc::Kind::FindValueResponse(_) => if *rpc_filter != RpcFilter::FindValueResponse { continue; },
               }
            }

            if let Some(ref sender_filter) = self.sender_filter {
               if !sender_filter.contains(&rpc.sender_id) {
                  continue;
               }
            }

            return Some(rpc);
         }
      }
      None
   }
}

