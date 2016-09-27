use {hash, node, routing, storage, rpc, bus, time, SubotaiError, SubotaiResult};
use std::{net, sync, cmp};
use rpc::Rpc;
use hash::SubotaiHash;
use node::receptions;
use std::str::FromStr;

/// Node resources for synchronous operations.
///
/// All methods on this module are synchronous, and will wait for any
/// remote nodes queried to reply to the RPCs sent, up to the timeout
/// defined at `node::self.configuration.network_timeout_s`. Complex operations 
/// involving multiple nodes have longer timeouts derived from that value.
/// The node layer above is in charge of parallelizing those operations 
/// by spawning threads when adequate.
pub struct Resources {
   pub id                : SubotaiHash,
   pub table             : routing::Table,
   pub storage           : storage::Storage,
   pub outbound          : net::UdpSocket,
   pub inbound           : net::UdpSocket,
   pub reception_updates : sync::Mutex<bus::Bus<ReceptionUpdate>>,
   pub network_updates   : sync::Mutex<bus::Bus<NetworkUpdate>>,
   pub state_updates     : sync::Mutex<bus::Bus<StateUpdate>>,
   pub conflicts         : sync::Mutex<Vec<routing::EvictionConflict>>,
   pub configuration     : node::Configuration,
   pub state             : sync::RwLock<node::State>,
}

/// Updates for the reception iterators. Mainly involves RPC received updates,
/// but keeps a constant tick to allow for timeouts and notifies of state changes
/// to fully abort the reception iterators if necessary.
#[derive(Clone, Debug)]
pub enum ReceptionUpdate {
   Tick,
   RpcReceived(Rpc),
   StateChange(node::State),
}

/// Notifies of new nodes entering the network and changes of state.
#[derive(Clone, Debug)]
pub enum NetworkUpdate {
   AddedNode(routing::NodeInfo),
   StateChange(node::State),
}

/// Just notifies about state changes.
#[derive(Clone, Debug)]
pub enum StateUpdate {
   StateChange(node::State),
}

impl Resources {
   pub fn local_info(&self) -> routing::NodeInfo {
      routing::NodeInfo {
         id      : self.id.clone(),
         address : self.inbound.local_addr().unwrap(),
      }
   }

   /// Current state of the node
   pub fn state(&self)-> node::State {
      *self.state.read().unwrap()
   }

   /// Changes node state and broadcasts the change
   pub fn set_state(&self, state: node::State) {
      { // Lock scope
         *self.state.write().unwrap() = state;
      }
      self.reception_updates.lock().unwrap().broadcast(ReceptionUpdate::StateChange(state));
      self.network_updates.lock().unwrap().broadcast(NetworkUpdate::StateChange(state));
      self.state_updates.lock().unwrap().broadcast(StateUpdate::StateChange(state));
   }

   /// Pings a node via its IP address, blocking until ping response.
   pub fn ping(&self, target: &net::SocketAddr) -> SubotaiResult<()> {
      let rpc = Rpc::ping(self.local_info());
      let packet = rpc.serialize();
      let responses = self.receptions()
         .during(time::Duration::seconds(self.configuration.network_timeout_s))
         .of_kind(receptions::KindFilter::PingResponse)
         .filter(|rpc| rpc.sender.address.ip() == target.ip() || 
                       target.ip() == net::IpAddr::from_str("0.0.0.0").unwrap())
         .take(1);
      try!(self.outbound.send_to(&packet, target));

      match responses.count() {
         1 => Ok(()),
         _ => Err(SubotaiError::NoResponse),
      }
   }

   /// Sends a ping and doesn't wait for a response. Used by the maintenance threads.
   pub fn ping_and_forget(&self, target: &net::SocketAddr) -> SubotaiResult<()> {
      let rpc = Rpc::ping(self.local_info());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, target));
      Ok(())
   }

   /// ReceptionUpdates the table with a new node, and starts the conflict resolution mechanism
   /// if necessary.
   pub fn update_table(&self, info: routing::NodeInfo) {
      let defensive = { // Lock scope
         *self.state.read().unwrap() == node::State::Defensive
      };

      let update_result = self.table.update_node(info.clone());

      if let routing::UpdateResult::CausedConflict(conflict) = update_result {
         if defensive {
            self.table.revert_conflict(conflict);
         } else {
            let mut conflicts = self.conflicts.lock().unwrap();
            conflicts.push(conflict);
            if conflicts.len() == self.configuration.max_conflicts {
               self.set_state(node::State::Defensive);
            }
         }
      } else if let routing::UpdateResult::AddedNode = update_result {
         self.network_updates.lock().unwrap().broadcast(NetworkUpdate::AddedNode(info));
      }

      let off_grid = { // Lock scope
         *self.state.read().unwrap() == node::State::OffGrid
      };

      // We go on grid as soon as the network is big enough.
      if off_grid && self.table.len() > self.configuration.k_factor {
         self.set_state(node::State::OnGrid);
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
         if let Some(found) = responses.iter().filter_map(|rpc| rpc.successfully_located(target)).next() {
            return WaveStrategy::Halt(found);
         }

         // If we didn't find it in this wave, but a parallel process or a slow response did, we are done.
         if let Some(found) = self.table.specific_node(target) {
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
   /// Returns the closest K we learned from, regardless of whether or not they're alive.
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

   pub fn retrieve(&self, key: &SubotaiHash) -> SubotaiResult<Vec<storage::StorageEntry>> {
      // If the value is already present in our table, we are done early.
      if let Some(entries) = self.storage.retrieve(key) {
         return Ok(entries);
      }

      // We start with the closest K nodes we know about.
      let mut closest: Vec<_> = self.table
         .closest_nodes_to(key)
         .filter(|info| &info.id != &self.id)
         .take(self.configuration.k_factor)
         .collect();
      let seeds: Vec<_> = closest.iter().cloned().take(self.configuration.alpha).collect();
      let mut cache_candidate: Option<routing::NodeInfo> = None;

      let strategy = |responses: &[rpc::Rpc], queried: &[routing::NodeInfo]| -> WaveStrategy<Vec<storage::StorageEntry>> {
         // If any parallel process, or the response from a slow node has retrieved the key,
         // we need to break out early
         if let Some(retrieved) = self.storage.retrieve(key) {
            return WaveStrategy::Halt(retrieved);
         }
         // We are interested in the combination of the nodes we knew about, plus the ones
         // we just learned from the responses, as long as we haven't queried them already.
         let mut former_closest = Vec::<routing::NodeInfo>::new();
         former_closest.append(&mut closest);
         closest = responses
            .iter()
            .filter_map(|rpc| rpc.is_helping_retrieve(key))
            .flat_map(|vec| vec.into_iter())
            .chain(former_closest)
            .filter(|info| !queried.contains(info) && &info.id != &self.id)
            .collect();
         closest.sort_by(|ref info_a, ref info_b| (&info_a.id ^ key).cmp(&(&info_b.id ^ key)));
         closest.dedup();

         // The cache candidate is the closest node that hasn't found the value.
         cache_candidate = closest.first().cloned();
       
         // If we found it, we cache the values and we're done.
         if let Some(retrieved) = responses.iter().filter_map(|rpc| rpc.successfully_retrieved(key)).next() {
            if let Some(ref candidate) = cache_candidate {
               let expiration = self.calculate_cache_expiration(&candidate.id, &key);
               for entry in &retrieved {
                  let rpc = Rpc::store(self.local_info(), key.clone(), entry.clone(), rpc::SerializableTime::from(expiration));
                  let packet = rpc.serialize();
                  let _ = self.outbound.send_to(&packet, candidate.address);
               }
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
  
   ///// the expiration time drops substantially the further away the parent node is from the key, past
   ///// a threshold.
   fn calculate_cache_expiration(&self, candidate_id: &SubotaiHash, key: &SubotaiHash) -> time::Tm {
      let distance = (candidate_id ^ key).height().unwrap_or(0);
      let adjusted_distance  = usize::saturating_sub(distance, self.configuration.expiration_distance_threshold) as u32;
      let clamped_distance = cmp::min(16, adjusted_distance);
      let expiration_factor = 2i64.pow(clamped_distance);
      time::now() + time::Duration::minutes(self.configuration.base_cache_time_mins / expiration_factor)
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
      
      try!(self.prune_bucket(index));

      let id = SubotaiHash::random_at_distance(&self.id, index);
      try!(self.probe(&id, self.configuration.k_factor));
      Ok(())
   }

   /// Pings all nodes in a bucket and eliminates unresponsive ones.
   pub fn prune_bucket(&self, index: usize) -> SubotaiResult<()>  {
      let mut nodes = self.table.nodes_from_bucket(index);
      let ids: Vec<_> = nodes.iter().map(|node| &node.id).cloned().collect();
      let responses = self
         .receptions()
         .of_kind(receptions::KindFilter::PingResponse)
         .during(time::Duration::seconds(self.configuration.network_timeout_s))
         .filter(|rpc| ids.contains(&rpc.sender.id))
         .take(ids.len());

      for node in self.table.nodes_from_bucket(index) {
         try!(self.ping_and_forget(&node.address));
      }
      
      for response in responses {
         nodes.retain(|node| node.id != response.sender.id);
      }

      for unresponsive_node in nodes {
         self.table.remove_node(&unresponsive_node.id);
      }

      Ok(())
   }

   /// Stores entries associated to a key with a single RPC.
   pub fn mass_store(&self, key: SubotaiHash, entries: Vec<(storage::StorageEntry, time::Tm)>) -> SubotaiResult<()> {
      if let node::State::OffGrid = *self.state.read().unwrap() {
         return Err(SubotaiError::OffGridError);
      }
      let storage_candidates = try!(self.probe(&key, self.configuration.k_factor));
      let cloned_key = key.clone();

      // At least one third of the store RPCs must succeed.
      let responses = self
         .receptions()
         .of_kind(receptions::KindFilter::StoreResponse)
         .during(time::Duration::seconds(self.configuration.network_timeout_s))
         .filter(|rpc| rpc.successfully_stored(&cloned_key))
         .take(self.configuration.k_factor / 3);
      
      let collection: Vec<_> = entries.into_iter().map(|(entry, time)| (entry, rpc::SerializableTime::from(time))).collect();
      let rpc = Rpc::mass_store(self.local_info(), key, collection );
      let packet = rpc.serialize();

      for candidate in &storage_candidates {
         try!(self.outbound.send_to(&packet, candidate.address));
      }

      if responses.count() == self.configuration.k_factor / 3 {
         Ok(())
      } else {
         Err(SubotaiError::UnresponsiveNetwork)
      }
   }

   pub fn store(&self, key: SubotaiHash, entry: storage::StorageEntry, expiration: time::Tm) -> SubotaiResult<()> {
      if let node::State::OffGrid = *self.state.read().unwrap() {
         return Err(SubotaiError::OffGridError);
      }

      let storage_candidates = try!(self.probe(&key, self.configuration.k_factor));
      let cloned_key = key.clone();

      // At least one third of the store RPCs must succeed.
      let responses = self
         .receptions()
         .of_kind(receptions::KindFilter::StoreResponse)
         .during(time::Duration::seconds(self.configuration.network_timeout_s))
         .filter(|rpc| rpc.successfully_stored(&cloned_key))
         .take(self.configuration.k_factor / 3);

      let rpc = Rpc::store(self.local_info(), key, entry, rpc::SerializableTime::from(expiration));
      let packet = rpc.serialize();

      for candidate in &storage_candidates {
         try!(self.outbound.send_to(&packet, candidate.address));
      }

      if responses.count() == self.configuration.k_factor / 3 {
         Ok(())
      } else {
         Err(SubotaiError::UnresponsiveNetwork)
      }
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
         rpc::Kind::LocateResponse(ref payload)    => self.handle_locate_response(payload.clone()),
         rpc::Kind::Probe(ref payload)             => self.handle_probe(payload.clone(), sender),
         rpc::Kind::Store(ref payload)             => self.handle_store(payload.clone(), sender),
         rpc::Kind::MassStore(ref payload)         => self.handle_mass_store(payload.clone(), sender),
         rpc::Kind::Retrieve(ref payload)          => self.handle_retrieve(payload.clone(), sender),
         rpc::Kind::RetrieveResponse(ref payload)  => self.handle_retrieve_response(payload.clone()),
         _ => Ok(()),
      };
      self.update_table(rpc.sender.clone());
      self.reception_updates.lock().unwrap().broadcast(ReceptionUpdate::RpcReceived(rpc));
      result
   }

   fn handle_ping(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      let rpc = Rpc::ping_response(self.local_info());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_store(&self, payload: sync::Arc<rpc::StorePayload>,  sender: routing::NodeInfo) -> SubotaiResult<()> {
      let store_result = self.storage.store(&payload.key, 
                                            &payload.entry,
                                            &time::Tm::from(payload.expiration.clone()));
      let rpc = Rpc::store_response(self.local_info(), payload.key.clone(), store_result);
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));

      Ok(())
   }

   fn handle_mass_store(&self, payload: sync::Arc<rpc::MassStorePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      
      let store_result = if payload.entries_and_expirations.iter().all(|&(ref entry, ref expiration)| {
         self.storage.store(&payload.key, &entry, &time::Tm::from(expiration.clone())) == storage::StoreResult::Success
      }) { storage::StoreResult::Success } else { storage::StoreResult::MassStoreFailed };

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

   fn handle_locate_response(&self, payload: sync::Arc<rpc::LocateResponsePayload>) -> SubotaiResult<()> {
      if let routing::LookupResult::Found(ref node) = payload.result {
         // This is an exception to the otherwise enforced rule of only introducing live nodes to
         // the table. Nodes we learn from through a locate query must always be inserted in the routing
         // table, so they can be picked up by other locate waves running in parallel.
         self.update_table(node.clone());
      }
      Ok(())
   }

   fn handle_retrieve_response(&self, payload: sync::Arc<rpc::RetrieveResponsePayload>) -> SubotaiResult<()> {
      if let rpc::RetrieveResult::Found(ref entries) = payload.result {
         // Retrieved keys are cached locally for a limited time, to guarantee succesive retrieves don't flood the network.
         for entry in entries {
            self.storage.store(&payload.key_to_find, entry, &(time::now() + time::Duration::minutes(1)));
         }
      }
      Ok(())
   }
}

enum WaveStrategy<T> {
   Continue(Vec<routing::NodeInfo>),
   Halt(T),
}

