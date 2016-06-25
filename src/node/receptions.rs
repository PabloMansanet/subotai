use bus;
use rpc;
use time;
use node::resources;

/// A blocking iterator over the RPCs received by a node.
pub struct Receptions {
   iter      : bus::BusIntoIter<resources::Update>,
   timeout   : Option<time::SteadyTime>,
}

impl Receptions {
   pub fn new(resources: &resources::Resources) -> Receptions {
      Receptions {
         iter    : resources.updates.lock().unwrap().add_rx().into_iter(),
         timeout : None,
      }
   }

   /// Restricts the iterator to a particular span of time.
   pub fn during(mut self, lifespan: time::Duration) -> Receptions {
      self.timeout = Some(time::SteadyTime::now() + lifespan);
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
            return Some(rpc);
         }
      }
      None
   }
}


