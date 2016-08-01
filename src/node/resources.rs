use {node, routing, rpc, bus, time, SubotaiError, SubotaiResult, hash};
use std::{net, sync};
use rpc::Rpc;
use hash::Hash;
use node::receptions;

/// Node resources for synchronous operations.
///
/// All methods on this module are synchronous, and will wait for any
/// remote nodes queried to reply to the RPCs sent, up to the timeout
/// defined at `node::node::NETWORK_TIMEOUT_S`. The node layer above is in 
/// charge of parallelizing those operations by spawning threads when
/// adequate.
pub struct Resources {
   pub id        : Hash,
   pub table     : routing::Table,
   pub outbound  : net::UdpSocket,
   pub inbound   : net::UdpSocket,
   pub state     : sync::Mutex<node::State>,
   pub updates   : sync::Mutex<bus::Bus<Update>>,
   pub conflicts : sync::Mutex<Vec<routing::EvictionConflict>>,
}

#[derive(Clone)]
pub enum Update {
   RpcReceived(Rpc),
   Tick,
   Shutdown,
}

impl Resources {
   pub fn local_info(&self) -> routing::NodeInfo {
      routing::NodeInfo {
         id      : self.id.clone(),
         address : self.inbound.local_addr().unwrap(),
      }
   }

   /// Pings a node, blocking until ping response.
   pub fn ping(&self, id: Hash) -> SubotaiResult<()> {
      let node = try!(self.find_node(&id));
      let rpc = Rpc::ping(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let packet = rpc.serialize();
      let responses = self.receptions()
         .during(time::Duration::seconds(node::NETWORK_TIMEOUT_S))
         .of_kind(receptions::KindFilter::PingResponse)
         .from(id.clone())
         .take(1);
      try!(self.outbound.send_to(&packet, node.address));

      match responses.count() {
         1 => Ok(()),
         _ => Err(SubotaiError::NoResponse),
      }
   }

   /// Sends a ping and doesn't wait for a response. Used by the maintenance thread
   /// and for conflict resolution.
   pub fn ping_and_forget(&self, id: Hash) -> SubotaiResult<()> {
      let node = try!(self.find_node(&id));
      let rpc = Rpc::ping(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, node.address));

      Ok(())
   }

   /// Updates the table with a new node, and starts the conflict resolution mechanism
   /// if necessary.
   pub fn update_table(&self, info: routing::NodeInfo) {
      match self.table.update_node(info) {
         routing::UpdateResult::CausedConflict(_) => unimplemented!(),
         _ => (),
      }
   }

   /// Attempts to find a node through the network.
   pub fn find_node(&self, id_to_find: &Hash) -> SubotaiResult<routing::NodeInfo> {
      // If the node is already present in our table, we are done early.
      if let Some(node) = self.table.specific_node(id_to_find) {
         return Ok(node);
      }

      let mut queried_ids = Vec::<Hash>::with_capacity(routing::K);
      let all_receptions = self.receptions();
      let loop_timeout = time::Duration::seconds(2 * node::NETWORK_TIMEOUT_S);
      let deadline = time::SteadyTime::now() + loop_timeout;
     
      while queried_ids.len() < routing::K && time::SteadyTime::now() < deadline {
         // We query the 'ALPHA' nodes closest to the target we haven't yet queried.
         let nodes_to_query: Vec<routing::NodeInfo> = self.table.closest_nodes_to(id_to_find)
            .filter(|ref info| !queried_ids.contains(&info.id))
            .take(routing::ALPHA)
            .collect();

         // We wait for the response from the same number of nodes, minus the 'IMPATIENCE' factor.
         let responses = self.receptions()
            .during(time::Duration::seconds(node::NETWORK_TIMEOUT_S))
            .filter(|ref rpc| rpc.is_finding_node(id_to_find))
            .take(usize::saturating_sub(nodes_to_query.len(), routing::IMPATIENCE));
        
         // We compose the RPCs and send the UDP packets.
         try!(self.lookup_wave(id_to_find, &nodes_to_query, &mut queried_ids));
  
         // We check the responses and return if the node was found.
         for response in responses { 
            println!("<-- Response from {}", response.sender_id);
            if response.found_node(id_to_find) {
               return self.table.specific_node(id_to_find).ok_or(SubotaiError::NodeNotFound);
            }
         }
      }
       
      // One last wait until success or timeout, to compensate for impatience
      // (It could be that the nodes we ignored earlier come back with the response)
      all_receptions.during(time::Duration::seconds(node::NETWORK_TIMEOUT_S))
         .filter(|ref rpc| rpc.found_node(id_to_find))
         .take(1)
         .count();
      self.table.specific_node(id_to_find).ok_or(SubotaiError::NodeNotFound)
   }

   pub fn bootstrap(&self, seed: routing::NodeInfo, network_size: Option<usize>) -> SubotaiResult<()>  {
      self.table.update_node(seed);

      // Timeout for the entire operation.
      let total_timeout = time::Duration::seconds(2 * node::NETWORK_TIMEOUT_S);
      let deadline = time::SteadyTime::now() + total_timeout;
      let mut responses = self.receptions()
         .of_kind(receptions::KindFilter::BootstrapResponse)
         .during(total_timeout);
       
      // We want our network to be as big as the K factor, or the user supplied limit.
      let expected_length = match network_size {
         Some(size) => size,
         None => routing::K,
      };

      let mut queried_ids = Vec::<Hash>::with_capacity(routing::K);
      while queried_ids.len() < expected_length {
         let nodes_to_query: Vec<routing::NodeInfo> = self.table.all_nodes()
            .filter(|ref node_info| !queried_ids.contains(&node_info.id))
            .collect();

         try!(self.bootstrap_wave(&nodes_to_query, &mut queried_ids));
         responses.next();

         if time::SteadyTime::now() >= deadline {
            return Err(SubotaiError::UnresponsiveNetwork);
         }
      }
      Ok(())
   }

   fn bootstrap_wave(&self, nodes_to_query: &[routing::NodeInfo], queried: &mut Vec<Hash>) -> SubotaiResult<()> {
      let rpc = Rpc::bootstrap(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let packet = rpc.serialize(); 
      for node in nodes_to_query {
         try!(self.outbound.send_to(&packet, node.address));
         queried.push(node.id.clone());
      }
      Ok(())
   }

   fn lookup_wave(&self, id_to_find: &Hash, nodes_to_query: &[routing::NodeInfo], queried: &mut Vec<Hash>) -> SubotaiResult<()> {
      let rpc = Rpc::find_node(
         self.id.clone(), 
         self.inbound.local_addr().unwrap().port(),
         id_to_find.clone(),
         routing::K,
      );
      let packet = rpc.serialize(); 
      for node in nodes_to_query {
         println!("--> Sending to {}", node.id);
         try!(self.outbound.send_to(&packet, node.address));
         queried.push(node.id.clone());
      }

      Ok(())
   }

   pub fn revert_conflicts_for_sender(&self, sender_id: &Hash) {
      let conflicts = self.conflicts.lock().unwrap();
      let matched_conflict = conflicts
         .iter()
         .enumerate()
         .filter(|&(_,&routing::EvictionConflict{ref evicted, ..})| sender_id == &evicted.id )
         .next();

      if let Some(index, _) = matched_conflict{
         let conflict = conflicts.remove(index);
         self.table.revert_conflict(conflict);
      }
   }

   pub fn process_incoming_rpc(&self, rpc: Rpc, mut source: net::SocketAddr) -> SubotaiResult<()>{
      source.set_port(rpc.reply_port);
      let sender = routing::NodeInfo {
         id      : rpc.sender_id.clone(),
         address : source,
      };

      let result = match rpc.kind {
         rpc::Kind::Ping                           => self.handle_ping(sender),
         rpc::Kind::PingResponse                   => self.handle_ping_response(sender),
         rpc::Kind::FindNode(ref payload)          => self.handle_find_node(payload.clone(), sender),
         rpc::Kind::FindNodeResponse(ref payload)  => self.handle_find_node_response(payload.clone(), sender),
         rpc::Kind::Bootstrap                      => self.handle_bootstrap(sender),
         rpc::Kind::BootstrapResponse(ref payload) => self.handle_bootstrap_response(payload.clone(),sender),
         _ => unimplemented!(),
      };

      self.updates.lock().unwrap().broadcast(Update::RpcReceived(rpc));
      result
   }

   fn handle_ping(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.update_node(sender.clone());
      let rpc = Rpc::ping_response(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_bootstrap(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.update_node(sender.clone());
      let closest_to_sender: Vec<_> = self.table.closest_nodes_to(&sender.id)
         .filter(|ref info| &info.id != &sender.id) // We don't want to reply with the sender itself
         .take(routing::K)
         .collect();

      let rpc = Rpc::bootstrap_response(self.id.clone(), self.inbound.local_addr().unwrap().port(), closest_to_sender);
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_bootstrap_response(&self, payload: sync::Arc<rpc::BootstrapResponsePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.update_node(sender.clone());
      for node in &payload.nodes {
         self.table.update_node(node.clone());
      }
      Ok(())
   }

   fn handle_ping_response(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.update_node(sender);
      Ok(())
   }

   fn handle_find_node(&self, payload: sync::Arc<rpc::FindNodePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.update_node(sender.clone());
      let lookup_results = self.table.lookup(&payload.id_to_find, payload.nodes_wanted, None);
      let rpc = Rpc::find_node_response(self.id.clone(), 
                                        self.inbound.local_addr().unwrap().port(),
                                        payload.id_to_find.clone(),
                                        lookup_results);
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_find_node_response(&self, payload: sync::Arc<rpc::FindNodeResponsePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.update_node(sender);
      match payload.result {
         routing::LookupResult::ClosestNodes(ref nodes) => for node in nodes { self.table.update_node(node.clone()); },
         routing::LookupResult::Found(ref node) => { self.table.update_node(node.clone()); },
         _ => (),
      }
      Ok(())
   }
}
