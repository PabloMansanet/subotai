use {hash, node, routing, storage, rpc, bus, time, SubotaiError, SubotaiResult};
use std::{net, sync, cmp};
use rpc::Rpc;
use hash::SubotaiHash;
use node::receptions;

/// Node resources for synchronous operations.
///
/// All methods on this module are synchronous, and will wait for any
/// remote nodes queried to reply to the RPCs sent, up to the timeout
/// defined at `node::self.configuration.network_timeout_s`. Complex operations 
/// involving multiple nodes have longer timeouts derived from that value.
/// The node layer above is in charge of parallelizing those operations 
/// by spawning threads when adequate.
pub struct Resources {
   pub id            : SubotaiHash,
   pub table         : routing::Table,
   pub storage       : storage::Storage,
   pub outbound      : net::UdpSocket,
   pub inbound       : net::UdpSocket,
   pub state         : sync::RwLock<node::State>,
   pub updates       : sync::Mutex<bus::Bus<Update>>,
   pub conflicts     : sync::Mutex<Vec<routing::EvictionConflict>>,
   pub configuration : node::Configuration,
}

#[derive(Clone)]
pub enum Update {
   RpcReceived(Rpc),
   Tick,
   StateChange(node::State),
}

impl Resources {
   pub fn local_info(&self) -> routing::NodeInfo {
      routing::NodeInfo {
         id      : self.id.clone(),
         address : self.inbound.local_addr().unwrap(),
      }
   }

   /// Pings a node, blocking until ping response.
   pub fn ping(&self, target: &routing::NodeInfo) -> SubotaiResult<()> {
      let rpc = Rpc::ping(self.local_info());
      let packet = rpc.serialize();
      let responses = self.receptions()
         .during(time::Duration::seconds(self.configuration.network_timeout_s))
         .of_kind(receptions::KindFilter::PingResponse)
         .from(target.id.clone())
         .take(1);
      try!(self.outbound.send_to(&packet, target.address));

      match responses.count() {
         1 => Ok(()),
         _ => Err(SubotaiError::NoResponse),
      }
   }

   /// Sends a ping and doesn't wait for a response. Used by the maintenance threads.
   pub fn ping_and_forget(&self, target: &routing::NodeInfo) -> SubotaiResult<()> {
      let rpc = Rpc::ping(self.local_info());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, target.address));
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
            if conflicts.len() == self.configuration.max_conflicts {
               *self.state.write().unwrap() = node::State::Defensive;
               self.updates.lock().unwrap().broadcast(Update::StateChange(node::State::Defensive));
            }
         }
      }

      let off_grid = { // Lock scope
         *self.state.read().unwrap() == node::State::OffGrid
      };

      // We go on grid as soon as the network is big enough.
      if off_grid && self.table.len() > self.configuration.k_factor {
         *self.state.write().unwrap() = node::State::OnGrid;
         self.updates.lock().unwrap().broadcast(Update::StateChange(node::State::OnGrid));
      }
   }

   /// Attempts to find a node through the network. This procedure will end as soon
   /// as the node is found, and will try to minimize network traffic while searching for it.
   /// It is also possible that the node will discard some of the intermediate nodes due
   /// to size concerns.
   ///
   /// For a more thorough mapping of the surroundings of a node, or if you specifically 
   /// need to know the K closest nodes to a given ID, use probe.
   pub fn locate(&self, target: &SubotaiHash) -> SubotaiResult<routing::NodeInfo> {
      // If the node is already present in our table, we are done early.
      if let Some(node) = self.table.specific_node(target) {
         return Ok(node);
      }

      let mut closest: Vec<_> = self.table.closest_nodes_to(target)
         .filter(|info| &info.id != &self.id)
         .take(self.configuration.k_factor)
         .collect();
      let seeds: Vec<_> = closest.iter().cloned().take(self.configuration.alpha).collect();

      // We use a wave operation to locate the node. We want to stop the wave if we
      // found the node, and to always contact the closest ALPHA nodes we have knowledge
      // of. We define a strategy method for such a wave.
      let strategy = |responses: &[rpc::Rpc], queried: &[routing::NodeInfo]| -> WaveStrategy<routing::NodeInfo> {
         // If we found it, we're done.
         if let Some(found) = responses.iter().filter_map(|rpc| rpc.successfuly_located(target)).next() {
            return WaveStrategy::Halt(found);
         }

         // We are interested in the combination of the nodes we knew about, plus the ones
         // we just learned from the responses, as long as we haven't queried them already.
         let mut former_closest = Vec::<routing::NodeInfo>::new();
         former_closest.append(&mut closest);
         closest = responses
            .iter()
            .filter_map(|rpc| rpc.is_helping_locate(target))
            .flat_map(|vec| vec.into_iter())
            .chain(former_closest)
            .collect();
       
         // We restore the order and remove duplicates, to finally return the closest ALPHA.
         closest.sort_by(|ref info_a, ref info_b| (&info_a.id ^ target).cmp(&(&info_b.id ^ target)));
         closest.dedup();
         WaveStrategy::Continue(closest
            .iter()
            .filter(|info| !queried.contains(info) && &info.id != &self.id)
            .cloned().take(self.configuration.alpha).collect()
         )
      };

      let rpc = Rpc::locate(self.local_info(), target.clone());
      let timeout = time::Duration::seconds(3*self.configuration.network_timeout_s);

      self.wave(seeds, strategy, rpc, timeout)
   }


   /// Thoroughly searches for the nodes closest to a given ID, returning the `K_FACTOR` closest.
   /// Returns the closest K we learned from, regardless of whether we have actually checked their
   /// if they are alive.
   ///
   /// The probe will consult `depth` number of nodes to obtain that information.
   pub fn probe(&self, target: &SubotaiHash, depth: usize) -> SubotaiResult<Vec<routing::NodeInfo>> {
      // We record the fact we attempted a probe for this bucket.
      self.table.mark_bucket_as_probed(target);

      // We start with the closest K nodes we know about.
      let mut closest: Vec<_> = self.table
         .closest_nodes_to(target)
         .filter(|info| &info.id != &self.id)
         .take(self.configuration.k_factor)
         .collect();

      let seeds: Vec<_> = closest.iter().cloned().take(self.configuration.alpha).collect();
      // Strategy is similar to the `locate` wave. We keep probing the closest `ALPHA` nodes
      // we are aware of as we continue probing. We only halt when we have queried `K_FACTOR`.
      let strategy = |responses: &[rpc::Rpc], queried: &[routing::NodeInfo]| -> WaveStrategy<Vec<routing::NodeInfo>> {
         let mut former_closest = Vec::<routing::NodeInfo>::new();
         former_closest.append(&mut closest);
         closest = responses
            .iter()
            .filter_map(|rpc| rpc.is_probe_response(target))
            .flat_map(|vec| vec.into_iter())
            .chain(former_closest)
            .collect();
       
         // We restore the order and remove duplicates, to finally return the closest ALPHA.
         closest.sort_by(|ref info_a, ref info_b| (&info_a.id ^ target).cmp(&(&info_b.id ^ target)));
         closest.dedup();

         if queried.len() >= depth {
            WaveStrategy::Halt(closest.iter().cloned().take(self.configuration.k_factor).collect())
         } else {
            WaveStrategy::Continue(closest
               .iter()
               .filter(|info| !queried.contains(info) && &info.id != &self.id)
               .cloned().take(self.configuration.alpha).collect()
            )
         }
      };

      let rpc = Rpc::probe(self.local_info(), target.clone());
      let timeout = time::Duration::seconds(3*self.configuration.network_timeout_s);

      self.wave(seeds, strategy, rpc, timeout)
   }

   pub fn retrieve(&self, key: &SubotaiHash) -> SubotaiResult<storage::StorageEntry> {
      // If the value is already present in our table, we are done early.
      if let Some(entry) = self.storage.retrieve(key) {
         return Ok(entry);
      }

      // We start with the closest K nodes we know about.
      let mut closest: Vec<_> = self.table
         .closest_nodes_to(key)
         .filter(|info| &info.id != &self.id)
         .take(self.configuration.k_factor)
         .collect();
      let seeds: Vec<_> = closest.iter().cloned().take(self.configuration.alpha).collect();
      let mut cache_candidate: Option<routing::NodeInfo> = None;

      let strategy = |responses: &[rpc::Rpc], queried: &[routing::NodeInfo]| -> WaveStrategy<storage::StorageEntry> {
         // We are interested in the combination of the nodes we knew about, plus the ones
         // we just learned from the responses, as long as we haven't queried them already.
         let mut former_closest = Vec::<routing::NodeInfo>::new();
         former_closest.append(&mut closest);
         closest = responses
            .iter()
            .filter_map(|rpc| rpc.is_helping_locate(key))
            .flat_map(|vec| vec.into_iter())
            .chain(former_closest)
            .filter(|info| !queried.contains(info) && &info.id != &self.id)
            .collect();
         closest.sort_by(|ref info_a, ref info_b| (&info_a.id ^ key).cmp(&(&info_b.id ^ key)));
         closest.dedup();

         // The cache candidate is the closest node that hasn't found the value.
         cache_candidate = closest.first().cloned();
       
         // If we found it, we cache the value and we're done.
         if let Some(retrieved) = responses.iter().filter_map(|rpc| rpc.successfully_retrieved(key)).next() {
            if let Some(ref candidate) = cache_candidate {
               let _ = self.store_remotely(candidate, key.clone(), retrieved.clone());
            }
            return WaveStrategy::Halt(retrieved);
         }

         WaveStrategy::Continue(closest
            .iter()
            .filter(|info| !queried.contains(info) && &info.id != &self.id)
            .cloned().take(self.configuration.alpha).collect()
         )
      };

      let rpc = Rpc::retrieve(self.local_info(), key.clone());
      let timeout = time::Duration::seconds(3*self.configuration.network_timeout_s);

      self.wave(seeds, strategy, rpc, timeout)
   }
  

   /// Wave operation. Contacts nodes from a list by sending a specific RPC. Then, it 
   /// extracts new node candidates from their response by applying a strategy function.
   ///
   /// The strategy function takes a list of Rpc responses and the IDs contacted so far
   /// in the wave, outputs the next nodes to contact, and decides whether to stop 
   /// the wave by producing a Some(T) in its second return value.
   ///
   /// The wave terminates when when the strategy function provides no new nodes, when a 
   /// global timeout is reached, or when halt returns Some(T).
   fn wave<T, S>(&self, seeds: Vec<routing::NodeInfo>, mut strategy: S, rpc: rpc::Rpc, timeout: time::Duration) -> SubotaiResult<T>
      where S: FnMut(&[rpc::Rpc], &[routing::NodeInfo]) -> WaveStrategy<T> {

      let deadline = time::SteadyTime::now() + timeout;
      let mut nodes_to_query = seeds;
      let mut queried = Vec::<routing::NodeInfo>::new();
      let packet = rpc.serialize();

      // We loop as long as we haven't ran out of time and there is something to query.
      while time::SteadyTime::now() < deadline && !nodes_to_query.is_empty() {
         // Here, we only know who to listen to, for how long, and the number of 
         // responses. Whether or not a response is interesting is down to the 
         // strategy function.
         let senders: Vec<SubotaiHash> = nodes_to_query.iter().map(|info| &info.id).cloned().collect();
         let responses = self.receptions()
            .from_senders(senders)
            .during(time::Duration::seconds(self.configuration.network_timeout_s))
            .take(cmp::min(nodes_to_query.len(), usize::saturating_sub(self.configuration.alpha, self.configuration.impatience)));
      
         // We query all the nodes with the wave RPC, and collect the responses, 
         // ignoring any slackers based on the IMPATIENCE factor.
         for node in &nodes_to_query {
            try!(self.outbound.send_to(&packet, node.address));
         }
         queried.append(&mut nodes_to_query);
         let responses: Vec<_> = responses.collect();

         // We return early if Halt produces a value. Otherwise, we calculate the next
         // nodes to query and continue.
         match strategy(&responses, &queried) {
            WaveStrategy::Continue(nodes) => nodes_to_query = nodes,
            WaveStrategy::Halt(result) => return Ok(result),
         }
      }
      Err(SubotaiError::UnresponsiveNetwork)
   }

   /// Probes a random node in a bucket, refreshing it.
   pub fn refresh_bucket(&self, index: usize) -> SubotaiResult<()> {
      if index > hash::HASH_SIZE {
         return Err(SubotaiError::OutOfBounds);
      }

      let id = SubotaiHash::random_at_distance(&self.id, index);
      try!(self.probe(&id, self.configuration.k_factor));
      Ok(())
   }

   pub fn store(&self, key: &SubotaiHash, entry: &storage::StorageEntry) ->SubotaiResult<()> {
      if let node::State::OffGrid = *self.state.read().unwrap() {
         return Err(SubotaiError::OffGridError);
      }

      let storage_candidates = try!(self.probe(&key, self.configuration.k_factor));
      // At least one store RPC must succeed.
      let mut response = self
         .receptions()
         .of_kind(receptions::KindFilter::StoreResponse)
         .during(time::Duration::seconds(self.configuration.network_timeout_s))
         .filter(|rpc| rpc.successfully_stored(key))
         .take(1);

      for candidate in &storage_candidates {
         try!(self.store_remotely_and_forget(candidate, key.clone(), entry.clone()));
      }

      match response.next() {
         Some(_) => Ok(()),
         None    => Err(SubotaiError::UnresponsiveNetwork),
      }
   }

   /// Instructs a node to store a key_value pair.
   fn store_remotely(&self, target: &routing::NodeInfo, key: SubotaiHash, entry: storage::StorageEntry) -> SubotaiResult<storage::StoreResult> {
      let rpc = Rpc::store(self.local_info(), key, entry);
      let packet = rpc.serialize();
      let mut responses = self.receptions()
         .during(time::Duration::seconds(self.configuration.network_timeout_s))
         .of_kind(receptions::KindFilter::StoreResponse)
         .from(target.id.clone())
         .take(1);
      try!(self.outbound.send_to(&packet, target.address));

      if let Some(rpc) = responses.next() {
         if let rpc::Kind::StoreResponse(ref payload) = rpc.kind {
            return Ok(payload.result.clone());
         }
      }

      Err(SubotaiError::NoResponse)
   }

   fn store_remotely_and_forget(&self, target: &routing::NodeInfo, key: SubotaiHash, entry: storage::StorageEntry) -> SubotaiResult<()> {
      let rpc = Rpc::store(self.local_info(), key, entry);
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, target.address));
      Ok(())
   }

   fn retrieve_wave(&self, key_to_find: &SubotaiHash, nodes_to_query: &[routing::NodeInfo], queried: &mut Vec<SubotaiHash>) -> SubotaiResult<()> {
      let rpc = Rpc::retrieve(self.local_info(), key_to_find.clone());
      let packet = rpc.serialize(); 
      for node in nodes_to_query {
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

   pub fn process_incoming_rpc(&self, mut rpc: Rpc, source: net::SocketAddr) -> SubotaiResult<()>{
      rpc.sender.address.set_ip(source.ip());
      let sender = rpc.sender.clone();

      let result = match rpc.kind {
         rpc::Kind::Ping                           => self.handle_ping(sender),
         rpc::Kind::PingResponse                   => self.handle_ping_response(sender),
         rpc::Kind::Locate(ref payload)            => self.handle_locate(payload.clone(), sender),
         rpc::Kind::Probe(ref payload)             => self.handle_probe(payload.clone(), sender),
         rpc::Kind::Store(ref payload)             => self.handle_store(payload.clone(), sender),
         rpc::Kind::Retrieve(ref payload)          => self.handle_retrieve(payload.clone(), sender),
         rpc::Kind::RetrieveResponse(ref payload)  => self.handle_retrieve_response(payload.clone()),
         _ => Ok(()),
      };
      self.update_table(rpc.sender.clone());
      self.updates.lock().unwrap().broadcast(Update::RpcReceived(rpc));
      result
   }

   fn handle_ping(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      let rpc = Rpc::ping_response(self.local_info());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_store(&self, payload: sync::Arc<rpc::StorePayload>,  sender: routing::NodeInfo) -> SubotaiResult<()> {
      let store_result = self.storage.store(payload.key.clone(), payload.entry.clone());
      let rpc = Rpc::store_response(self.local_info(), payload.key.clone(), store_result);
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));

      Ok(())
   }

   fn handle_probe(&self, payload: sync::Arc<rpc::ProbePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      // We respond with K_FACTOR nodes plus one, because we might be including the identity of
      // the probing node, and the probing node is interested in K_FACTOR others.
      let closest: Vec<_> = self.table
         .closest_nodes_to(&payload.id_to_probe)
         .take(self.configuration.k_factor + 1)
         .collect();

      let rpc = Rpc::probe_response(self.local_info(),
                                    closest, 
                                    payload.id_to_probe.clone());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_ping_response(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.revert_conflicts_for_sender(&sender.id);
      Ok(())
   }

   fn handle_locate(&self, payload: sync::Arc<rpc::LocatePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      let lookup_results = self.table.lookup(&payload.id_to_find, self.configuration.k_factor, None);
      let rpc = Rpc::locate_response(self.local_info(),
                                     payload.id_to_find.clone(),
                                     lookup_results);
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_retrieve(&self, payload: sync::Arc<rpc::RetrievePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      let result = match self.storage.retrieve(&payload.key_to_find) {
         Some(value) => rpc::RetrieveResult::Found(value),
         None => rpc::RetrieveResult::Closest(self.table.closest_nodes_to(&payload.key_to_find).take(self.configuration.k_factor).collect()),
      };

      let rpc = Rpc::retrieve_response(self.local_info(),
                                       payload.key_to_find.clone(),
                                       result);
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_retrieve_response(&self, payload: sync::Arc<rpc::RetrieveResponsePayload>) -> SubotaiResult<()> {
      if let rpc::RetrieveResult::Found(ref value) = payload.result {
         self.storage.store(payload.key_to_find.clone(), value.clone());
      }
      Ok(())
   }
}

enum WaveStrategy<T> {
   Continue(Vec<routing::NodeInfo>),
   Halt(T),
}

