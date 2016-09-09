use {bus, rpc, time, node};
use node::resources;
use hash::SubotaiHash;

/// Iterator over all RPCs received by a node. 
///
/// By default, iterating over a Receptions object will block indefinitely
/// while waiting for packet arrivals, but it's possible to specify an
/// imprecise timeout so the iterator is only valid for a span of time.
///
/// It is also possible to filter the iterator so it only applies to particular
/// senders or RPC kinds without resorting to iterator adapters.
pub struct Receptions {
   iter          : bus::BusIntoIter<resources::Update>,
   timeout       : Option<time::SteadyTime>,
   kind_filter   : Option<KindFilter>,
   sender_filter : Option<Vec<SubotaiHash>>,
   shutdown      : bool,
}

/// Filters out all RPCs except those of a particular kind.
#[derive(Eq, PartialEq, Debug)]
pub enum KindFilter {
   Ping,
   PingResponse,
   Store,
   StoreResponse,
   Locate,
   LocateResponse,
   Retrieve,
   RetrieveResponse,
   Probe,
   ProbeResponse,
}

impl resources::Resources {
   pub fn receptions(&self) -> Receptions {
      Receptions::new(self)
   }
}

impl Receptions {
   fn new(resources: &resources::Resources) -> Receptions {
      Receptions {
         iter          : resources.updates.lock().unwrap().add_rx().into_iter(),
         timeout       : None,
         kind_filter   : None,
         sender_filter : None,
         shutdown      : false,
      }
   }

   /// Restricts the iterator to a particular span of time.
   pub fn during(mut self, lifespan: time::Duration) -> Receptions {
      self.timeout = Some(time::SteadyTime::now() + lifespan);
      self
   }

   /// Only produces a particular rpc kind.
   pub fn of_kind(mut self, filter: KindFilter) -> Receptions {
      self.kind_filter = Some(filter);
      self
   }

   /// Only from a sender.
   pub fn from(mut self, sender: SubotaiHash) -> Receptions {
      self.sender_filter = Some(vec![sender]);
      self
   }

   /// Only from a set of senders.
   pub fn from_senders(mut self, senders: Vec<SubotaiHash>) -> Receptions {
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
         if self.shutdown {
            break;
         }

         match self.iter.next() {
            Some(resources::Update::RpcReceived(rpc)) => {
               if let Some(ref kind_filter) = self.kind_filter {
                  match rpc.kind {
                     rpc::Kind::Ping                 => if *kind_filter != KindFilter::Ping { continue; },
                     rpc::Kind::PingResponse         => if *kind_filter != KindFilter::PingResponse { continue; },
                     rpc::Kind::Store(_)             => if *kind_filter != KindFilter::Store { continue; },
                     rpc::Kind::StoreResponse(_)     => if *kind_filter != KindFilter::StoreResponse { continue; },
                     rpc::Kind::Locate(_)            => if *kind_filter != KindFilter::Locate { continue; },
                     rpc::Kind::LocateResponse(_)    => if *kind_filter != KindFilter::LocateResponse { continue; },
                     rpc::Kind::Retrieve(_)          => if *kind_filter != KindFilter::Retrieve { continue; },
                     rpc::Kind::RetrieveResponse(_)  => if *kind_filter != KindFilter::RetrieveResponse { continue; },
                     rpc::Kind::Probe(_)             => if *kind_filter != KindFilter::Probe { continue; },
                     rpc::Kind::ProbeResponse(_)     => if *kind_filter != KindFilter::ProbeResponse { continue; },
                  }
               }

               if let Some(ref sender_filter) = self.sender_filter {
                  if !sender_filter.contains(&rpc.sender.id) {
                     continue;
                  }
               }

               return Some(rpc);
            },
            Some(resources::Update::StateChange(node::State::ShuttingDown)) => self.shutdown = true,
            _ => (),
         }
      }
      None
   }
}

#[cfg(test)]
mod tests {
    use node;
    use time;
    use super::KindFilter;

    #[test]
    fn produces_rpcs_but_not_ticks() {
       let alpha = node::Node::new().unwrap();
       let beta = node::Node::new().unwrap();
       alpha.bootstrap(&beta.resources.local_info().address).unwrap();

       assert_eq!(alpha.resources.table.len(), 2); // One for self, and one for beta
       let beta_receptions = beta
         .receptions()
         .during(time::Duration::seconds(1))
         .of_kind(KindFilter::Ping);

       assert!(alpha.resources.ping(&beta.local_info().address).is_ok());
       assert!(alpha.resources.ping(&beta.local_info().address).is_ok());

       assert_eq!(beta_receptions.count(),2);
    }

    #[test]
    fn sender_filtering() {
       let receiver = node::Node::new().unwrap();
       let alpha = node::Node::new().unwrap();
       let beta  = node::Node::new().unwrap();
       
       let mut allowed = Vec::new();
       allowed.push(beta.resources.local_info().id);
      
       let receptions = receiver
         .receptions()
         .during(time::Duration::seconds(1))
         .from_senders(allowed)
         .of_kind(KindFilter::Ping);

       assert!(receiver.bootstrap(&alpha.resources.local_info().address).is_ok());
       assert!(receiver.bootstrap(&beta.resources.local_info().address).is_ok());

       assert!(alpha.resources.ping(&receiver.local_info().address).is_ok());
       assert!(beta.resources.ping(&receiver.local_info().address).is_ok());

       assert_eq!(receptions.count(),1);
    }
}


