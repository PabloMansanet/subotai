use {node, routing, storage, rpc, bus, time, SubotaiError, SubotaiResult};
use std::{net, sync};
use rpc::Rpc;
use hash::SubotaiHash;
use node::receptions;

/// Node resources for synchronous operations.
///
/// All methods on this module are synchronous, and will wait for any
/// remote nodes queried to reply to the RPCs sent, up to the timeout
/// defined at `node::node::NETWORK_TIMEOUT_S`. The node layer above is in 
/// charge of parallelizing those operations by spawning threads when
/// adequate.
pub struct Resources {
   pub id        : SubotaiHash,
   pub table     : routing::Table,
   pub storage   : storage::Storage,
   pub outbound  : net::UdpSocket,
   pub inbound   : net::UdpSocket,
   pub state     : sync::RwLock<node::State>,
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
   pub fn ping(&self, id: &SubotaiHash) -> SubotaiResult<()> {
      let node = try!(self.find_node(id));
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

   /// Sends a ping and doesn't wait for a response. Used by the maintenance thread.
   pub fn ping_and_forget(&self, id: &SubotaiHash) -> SubotaiResult<()> {
      let node = try!(self.find_node(id));
      let rpc = Rpc::ping(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, node.address));
      Ok(())
   }

   /// Sends a ping to an evicted node not present in the routing table anymore. Used
   /// by the conflict resolution thread.
   pub fn ping_for_conflict(&self, evicted: &routing::NodeInfo) -> SubotaiResult<()> {
      let rpc = Rpc::ping(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, evicted.address));
      Ok(())
   }

   /// Updates the table with a new node, and starts the conflict resolution mechanism
   /// if necessary.
   pub fn update_table(&self, info: routing::NodeInfo) {
      let defensive = { // Lock scope
         *self.state.read().unwrap() == node::State::Defensive
      };

      if let routing::UpdateResult::CausedConflict(conflict) = self.table.update_node(info) {
         if defensive {
            self.table.revert_conflict(conflict);
         } else {
            let mut conflicts = self.conflicts.lock().unwrap();
            conflicts.push(conflict);
            if conflicts.len() == routing::MAX_CONFLICTS {
               *self.state.write().unwrap() = node::State::Defensive;
            }
         }
      }
   }

   /// Attempts to find a node through the network. This procedure will end as soon
   /// as the node is found, and will try to minimize network traffic while searching for it.
   /// It is also possible that the node will discard some of the intermediate nodes due
   /// to size concerns.
   ///
   /// For a more thorough mapping of the surroundings of a node, or if you specifically 
   /// need to know the K closest nodes to a given ID, use probe_node.
   pub fn find_node(&self, target: &SubotaiHash) -> SubotaiResult<routing::NodeInfo> {
      // If the node is already present in our table, we are done early.
      if let Some(node) = self.table.specific_node(target) {
         return Ok(node);
      }

      let mut queried_ids = Vec::<SubotaiHash>::with_capacity(routing::K_FACTOR);
      let all_receptions = self.receptions();
      let loop_timeout = time::Duration::seconds(3 * node::NETWORK_TIMEOUT_S);
      let deadline = time::SteadyTime::now() + loop_timeout;
     
      while queried_ids.len() < routing::K_FACTOR && time::SteadyTime::now() < deadline {
         // We query the 'ALPHA' nodes closest to the target we haven't yet queried.
         let nodes_to_query: Vec<routing::NodeInfo> = self.table.closest_nodes_to(target)
            .filter(|ref info| !queried_ids.contains(&info.id) && &info.id != &self.id)
            .take(routing::ALPHA)
            .collect();

         // We wait for the response from the same number of nodes, minus the 'IMPATIENCE' factor.
         let responses = self.receptions()
            .during(time::Duration::seconds(node::NETWORK_TIMEOUT_S))
            .filter(|ref rpc| rpc.is_finding_node(target))
            .take(usize::saturating_sub(nodes_to_query.len(), routing::IMPATIENCE));
        
         // We compose the RPCs and send the UDP packets.
         try!(self.lookup_wave(target, &nodes_to_query, &mut queried_ids));
  
         // We check the responses and return if the node was found.
         for response in responses { 
            println!("<-- Response from {}", response.sender_id);
            if response.found_node(target) {
               return self.table.specific_node(target).ok_or(SubotaiError::NodeNotFound);
            }
         }
      }
      
      // One last wait until success or timeout, to compensate for impatience
      // (It could be that the nodes we ignored earlier come back with the response)
      all_receptions
         .during(time::Duration::seconds(node::NETWORK_TIMEOUT_S))
         .filter(|ref rpc| rpc.found_node(target))
         .take(1)
         .count();
      self.table.specific_node(target).ok_or(SubotaiError::NodeNotFound)
   }

   /// Thoroughly searches for the nodes closest to a given ID, returning the 'K_FACTOR' closest.
   /// It is possible that not all of these nodes will be stored in the routing table, so use
   /// the return value of this function rather than a subsequent call for table.closest_nodes_to().
   pub fn probe_node(&self, target: &SubotaiHash) -> SubotaiResult<Vec<routing::NodeInfo>> {
      // We start with the closest K nodes we know about.
      let mut closest:Vec<routing::NodeInfo> = 
         self.table.closest_nodes_to(target)
                   .take(routing::K_FACTOR)
                   .collect();

      // We define a timeout for the entire operation.
      let total_timeout = time::Duration::seconds(3 * node::NETWORK_TIMEOUT_S);
      let deadline = time::SteadyTime::now() + total_timeout;

      // We keep track of the nodes we have already queried to avoid spam.
      let mut queried_ids = Vec::<SubotaiHash>::with_capacity(routing::K_FACTOR);

      while queried_ids.len() < routing::K_FACTOR {

         // Decide what nodes to query (The alpha closest we haven't queried yet).
         let nodes_to_query: Vec<routing::NodeInfo> = closest.iter()
            .filter(|info| !queried_ids.contains(&info.id) && &info.id != &self.id)
            .take(routing::ALPHA)
            .cloned()
            .collect();
         if nodes_to_query.is_empty() {
            break;
         }

         // We prepare for the probe responses, from the amount of nodes
         // we are contacting, minus the impatience factor.
         let responses = self.receptions()
            .of_kind(receptions::KindFilter::ProbeResponse)
            .during(time::Duration::seconds(node::NETWORK_TIMEOUT_S))
            .take(usize::saturating_sub(nodes_to_query.len(), routing::IMPATIENCE));

         // We probe these nodes.
         try!(self.probe_wave(target.clone(), &nodes_to_query, &mut queried_ids));

         // We incorporate the nodes we receive as responses, making sure to avoid ID duplicates.
         for response in responses {
            if let rpc::Kind::ProbeResponse(ref payload) = response.kind {
               let mut new_nodes = payload.nodes.clone();
               new_nodes.retain(|ref new| closest.iter().all(|ref old| &old.id != &new.id));
               closest.append(&mut new_nodes);
            }
         }
         
         // We sort the vector again to keep the ordering up to date, and slim it down to K_FACTOR entries.
         closest.sort_by(|ref info_a, ref info_b| (&info_a.id ^ target).cmp(&(&info_b.id ^ target)));
         closest.truncate(routing::K_FACTOR);

         if time::SteadyTime::now() >= deadline {
            return Err(SubotaiError::UnresponsiveNetwork);
         }
      }

      closest.shrink_to_fit();
      Ok(closest)
   }
   
   /// Locates a node in the network and instructs it to locate a key-value pair.
   pub fn store_remotely(&self, id: &SubotaiHash, key: SubotaiHash, value: SubotaiHash) -> SubotaiResult<()> {
      let node = try!(self.find_node(id));
      let rpc = Rpc::store(self.id.clone(), 
                           self.inbound.local_addr().unwrap().port(),
                           key,
                           value);
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, node.address));
      Ok(())
   }

   pub fn bootstrap(&self, seed: routing::NodeInfo, network_size: Option<usize>) -> SubotaiResult<()>  {
      self.update_table(seed);

      // Timeout for the entire operation.
      let total_timeout = time::Duration::seconds(3 * node::NETWORK_TIMEOUT_S);
      let deadline = time::SteadyTime::now() + total_timeout;
      let mut responses = self.receptions()
         .of_kind(receptions::KindFilter::ProbeResponse)
         .during(total_timeout);
       
      // We want our network to be as big as the K factor, or the user supplied limit.
      let expected_length = match network_size {
         Some(size) => size,
         None => routing::K_FACTOR,
      };

      let mut queried_ids = Vec::<SubotaiHash>::with_capacity(routing::K_FACTOR);
      while queried_ids.len() < expected_length {
         let nodes_to_query: Vec<routing::NodeInfo> = self.table.all_nodes()
            .filter(|ref node_info| !queried_ids.contains(&node_info.id))
            .collect();

         try!(self.probe_wave(self.id.clone(), &nodes_to_query, &mut queried_ids));
         responses.next();

         if time::SteadyTime::now() >= deadline {
            return Err(SubotaiError::UnresponsiveNetwork);
         }
      }
      Ok(())
   }

   fn probe_wave(&self, id_to_probe: SubotaiHash, nodes_to_query: &[routing::NodeInfo], queried: &mut Vec<SubotaiHash>) -> SubotaiResult<()> {
      let rpc = Rpc::probe(self.id.clone(), self.inbound.local_addr().unwrap().port(), id_to_probe);
      let packet = rpc.serialize(); 

      for node in nodes_to_query {
         try!(self.outbound.send_to(&packet, node.address));
         queried.push(node.id.clone());
      }
      Ok(())
   }

   fn lookup_wave(&self, id_to_find: &SubotaiHash, nodes_to_query: &[routing::NodeInfo], queried: &mut Vec<SubotaiHash>) -> SubotaiResult<()> {
      let rpc = Rpc::find_node(
         self.id.clone(), 
         self.inbound.local_addr().unwrap().port(),
         id_to_find.clone(),
         routing::K_FACTOR,
      );
      let packet = rpc.serialize(); 
      for node in nodes_to_query {
         println!("--> Sending to {}", node.id);
         try!(self.outbound.send_to(&packet, node.address));
         queried.push(node.id.clone());
      }
      Ok(())
   }

   pub fn revert_conflicts_for_sender(&self, sender_id: &SubotaiHash) {
      if let Some((index, _)) = 
         self.conflicts.lock().unwrap().iter()
         .enumerate()
         .find(|&(_,&routing::EvictionConflict{ref evicted, ..})| sender_id == &evicted.id )
      {
         let conflict = self.conflicts.lock().unwrap().remove(index);
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
         rpc::Kind::Probe(ref payload)             => self.handle_probe(payload.clone(), sender),
         rpc::Kind::ProbeResponse(ref payload)     => self.handle_probe_response(payload.clone(), sender),
         rpc::Kind::Store(ref payload)             => self.handle_store(payload.clone(), sender),
         _ => unimplemented!(),
      };
      
      self.updates.lock().unwrap().broadcast(Update::RpcReceived(rpc));
      result
   }

   fn handle_ping(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.update_table(sender.clone());
      let rpc = Rpc::ping_response(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_store(&self, payload: sync::Arc<rpc::StorePayload>,  sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.update_table(sender.clone());
      self.storage.store(payload.key.clone(), payload.value.clone());
      Ok(())
   }

   fn handle_probe(&self, payload: sync::Arc<rpc::ProbePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.update_table(sender.clone());
      let closest: Vec<_> = self.table.closest_nodes_to(&payload.id_to_probe)
         .take(routing::K_FACTOR)
         .collect();

      let rpc = Rpc::probe_response(self.id.clone(), 
                                    self.inbound.local_addr().unwrap().port(),
                                    closest, 
                                    payload.id_to_probe.clone());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_probe_response(&self, payload: sync::Arc<rpc::ProbeResponsePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.update_table(sender.clone());
      for node in &payload.nodes {
         self.update_table(node.clone());
      }
      Ok(())
   }

   fn handle_ping_response(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.revert_conflicts_for_sender(&sender.id);
      self.update_table(sender);
      Ok(())
   }

   fn handle_find_node(&self, payload: sync::Arc<rpc::FindNodePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.update_table(sender.clone());
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
      self.update_table(sender);
      match payload.result {
         routing::LookupResult::ClosestNodes(ref nodes) => for node in nodes { self.update_table(node.clone()); },
         routing::LookupResult::Found(ref node) => { self.update_table(node.clone()); },
         _ => (),
      }
      Ok(())
   }
}
