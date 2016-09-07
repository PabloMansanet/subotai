//! #Node
//!
//! The node module is the main point of contact with Subotai. Node structs contain
//! all the pieces needed to join a Subotai network.
//!
//! When you initialize a Node struct, a few threads are automatically launched that 
//! take care of listening to RPCs from other nodes, automatic maintenance and eviction
//! of older node entries, and more. 
//!
//! Nodes start in the `OffGrid` state by default, meaning they aren't associated to
//! a network. You must bootstrap the node by providing the address of another node,
//! at which point the state will change to `OnGrid`.
//!
//! Destroying a node automatically schedules all threads to terminate after finishing 
//! any pending operations.

/// Allows listening to RPCs received by a node. Unnecessary for normal operation,
/// but it can be useful for debugging your network.
pub mod receptions;
pub use routing::NodeInfo as NodeInfo;
pub use storage::StorageEntry as StorageEntry;
pub use node::factory::Factory as Factory;

#[cfg(test)]
mod tests;
mod resources;
mod factory;

use {storage, routing, rpc, bus, SubotaiResult, time};
use hash::SubotaiHash;
use std::{net, thread, sync};
use std::time::Duration as StdDuration;

/// Size of a typical UDP socket buffer.
pub const SOCKET_BUFFER_SIZE_BYTES : usize = 65536;
const SOCKET_TIMEOUT_MS     : u64   = 200;
const UPDATE_BUS_SIZE_BYTES : usize = 50;

/// Maintenance thread sleep period.
const MAINTENANCE_SLEEP_S : u64 = 5;

/// Attempts to probe self during the bootstrap process.
const BOOTSTRAP_TRIES : u32 = 3;

/// Subotai node. 
///
/// On construction, it launches three asynchronous threads.
///
/// * For packet reception.
/// * For conflict resolution.
/// * For general maintenance.
pub struct Node {
   resources: sync::Arc<resources::Resources>,
}

/// State of a Subotai node. 
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum State {
   /// The node is initialized but disconnected from the 
   /// network. Needs to go through succesful bootstrapping.
   OffGrid,

   /// The node is online and connected to the network.
   OnGrid,

   /// The node is in defensive mode. Too many conflicts have 
   /// been generated recently, so the node gives preference
   /// to its older contacts until all conflicts are resolved.
   Defensive,

   /// The node is in a process of shutting down;
   /// all of its resources will be deallocated after completion
   /// of any pending async operations.
   ShuttingDown,
}

/// Network configuration constants. Do not set these values directly, as there 
/// is no way to initialize a node from a `Configuration` struct. Instead, use 
/// `node::Factory` if you want your application to use non-default network constants.
///
/// Note that for the network to function optimally, the `alpha`, `impatience`, 
/// `expiration_distance_threshold` and  `base_expiration_time_hrs` must be identical 
/// for all nodes.
#[derive(Clone, Debug)]
pub struct Configuration {
   /// Network-wide concurrency factor. It's used, for example, to decide the
   /// number of remote nodes to interrogate concurrently when performing a 
   /// network-wide lookup.
   pub alpha                         : usize,

   /// Impatience factor, valid in the range [0..ALPHA). When performing "waves",
   /// the impatience factor denotes how many nodes we can give up waiting for, before
   /// starting the next wave. 
   ///
   /// If we send a request to ALPHA nodes during a lookup wave, we will start
   /// the next wave after we receive 'ALPHA - IMPATIENCE' responses.
   pub impatience                    : usize,

   /// Data structure factor. It's used to dictate the size of the internal routing
   /// data structures (k-buckets).
   pub k_factor                      : usize,

   /// Maximum amount of eviction conflicts allowed before the node goes into
   /// a temporary defensive mode, and starts to prioritize old contacts to new, 
   /// potentially malicious ones.
   pub max_conflicts                 : usize,

   /// Maximum amount of storage entries (key-value or key-blob pairs). This has no
   /// effect on the routing table size (amount of node id-address pairs), which is
   /// dictated by the k_factor.
   pub max_storage                   : usize,

   /// Maximum size in bytes for a blob storage entry. (A blob entry consists in a 
   /// key associated with a chunk of binary data, instead of a 160 bit value hash).
   pub max_storage_blob_size         : usize,

   /// Xor distance from a key at which point nodes will start to dramatically decrease
   /// the expiration time for cached storage entries. This is only relevant in cases of 
   /// extreme network traffic around a given key. A bigger threshold allows for more
   /// over-caching and therefore less load on the critical nodes, while a smaller 
   /// threshold allows for a leaner network.
   pub expiration_distance_threshold : usize,

   /// Base expiration time for storage entries. Every time you call `store` on a node
   /// that resides on a live network (i.e. is in an `OnGrid` state) you guarantee the
   /// entry will remain in the network for this number of hours. Calling `store` again
   /// will refresh this time.
   pub base_expiration_time_hrs      : i64,

   /// Time in seconds after which it can be assumed that a remote node has failed to 
   /// respond to a query.
   pub network_timeout_s             : i64,
}

impl Default for Configuration {
   fn default() -> Configuration {
      Configuration {
         alpha                         : 3,
         impatience                    : 1,
         k_factor                      : 20,
         max_conflicts                 : 60,
         max_storage                   : 10000,
         max_storage_blob_size         : 1024,
         expiration_distance_threshold : 3,
         base_expiration_time_hrs      : 24,
         network_timeout_s             : 5,
      }
   }
}

impl Node {
   /// Constructs a node with OS allocated random ports and default network constants.
   /// 
   /// If you need more control over ports and network configuration, use `node::Factory`.
   pub fn new() -> SubotaiResult<Node> {
      Node::with_configuration(0, 0, Default::default())
   }

   /// Stores an entry in the network, refreshing its expiration time back to the base value.
   pub fn store(&self, key: SubotaiHash, entry: StorageEntry) -> SubotaiResult<()> {
      let expiration = time::now() + time::Duration::hours(self.resources.configuration.base_expiration_time_hrs);
      self.resources.store(key, entry, expiration)
   }

   /// Retrieves all values associated to a key from the network.
   pub fn retrieve(&self, key: &SubotaiHash) -> SubotaiResult<Vec<StorageEntry>> {
      self.resources.retrieve(key)
   }

   /// Returns the hash used to identify this node in the network.
   pub fn id(&self) -> &SubotaiHash {
      &self.resources.id
   }

   /// Returns the network constant strucure for this node.
   pub fn configuration(&self) -> &Configuration {
      &self.resources.configuration
   }

   /// Returns the current state of the node.
   pub fn state(&self)-> State {
      *self.resources.state.read().unwrap()
   }

   /// Produces an iterator over RPCs received by this node. The iterator will block
   /// indefinitely.
   pub fn receptions(&self) -> receptions::Receptions {
      self.resources.receptions()
   }

   /// Bootstraps the node from a seed. Returns Ok(()) if the seed has
   /// been reached and the asynchronous bootstrap process has started.
   /// However, it might take a bit for the node to become alive (use 
   /// node::wait_until_state to block until it's alive, if necessary).
   pub fn bootstrap(&self, seed: NodeInfo) -> SubotaiResult<()> {
      try!(self.resources.ping(&seed));
      let bootstrap_resources = self.resources.clone();
      thread::spawn(move || {
         for _ in 0..BOOTSTRAP_TRIES {
            if let Ok(_) = bootstrap_resources.probe(&bootstrap_resources.id, bootstrap_resources.configuration.k_factor) {
               break;
            }
         }
       });
      Ok(())
   }

   /// Returns if the node is already in the specified state, otherwise blocks indefinitely until
   /// that state is reached.
   pub fn wait_for_state(&self, state: State) {
      let updates = self.resources.updates.lock().unwrap().add_rx().into_iter();
      if *self.resources.state.read().unwrap() == state {
         return;
      }

      for update in updates {
         match update {
            resources::Update::StateChange(new_state) if new_state == state => return,
            _ => (),
         }
      }
   }

   /// Retrieves the node ID + address pair.
   pub fn local_info(&self) -> NodeInfo {
      self.resources.local_info()
   }

   fn with_configuration(inbound_port: u16, outbound_port: u16, configuration: Configuration) -> SubotaiResult<Node> {
      let id = SubotaiHash::random();
      
      let resources = sync::Arc::new(resources::Resources {
         id            : id.clone(),
         table         : routing::Table::new(id.clone(), configuration.clone()),
         storage       : storage::Storage::new(id, configuration.clone()),
         inbound       : try!(net::UdpSocket::bind(("0.0.0.0", inbound_port))),
         outbound      : try!(net::UdpSocket::bind(("0.0.0.0", outbound_port))),
         state         : sync::RwLock::new(State::OffGrid),
         updates       : sync::Mutex::new(bus::Bus::new(UPDATE_BUS_SIZE_BYTES)),
         conflicts     : sync::Mutex::new(Vec::with_capacity(configuration.max_conflicts)),
         configuration : configuration,
      });

      resources.table.update_node(resources.local_info());

      try!(resources.inbound.set_read_timeout(Some(StdDuration::from_millis(SOCKET_TIMEOUT_MS))));

      let reception_resources = resources.clone();
      thread::spawn(move || { Node::reception_loop(reception_resources) });

      let conflict_resolution_resources = resources.clone();
      thread::spawn(move || { Node::conflict_resolution_loop(conflict_resolution_resources) });

      let maintenance_resources = resources.clone();
      thread::spawn(move || { Node::maintenance_loop(maintenance_resources) });

      Ok( Node{ resources: resources } )
   }

   /// Receives and processes data as long as the node is alive.
   fn reception_loop(resources: sync::Arc<resources::Resources>) {
      let mut buffer = [0u8; SOCKET_BUFFER_SIZE_BYTES];

      loop {
         let message = resources.inbound.recv_from(&mut buffer);
         if let State::ShuttingDown = *resources.state.read().unwrap() {
            break;
         }

         if let Ok((_, source)) = message {
            if let Ok(rpc) = rpc::Rpc::deserialize(&buffer) {
               let resources_clone = resources.clone();
               thread::spawn(move || { resources_clone.process_incoming_rpc(rpc, source) } );
            }
         }

         resources.updates.lock().unwrap().broadcast(resources::Update::Tick);
      }
   }

   /// Wakes up every `MAINTENANCE_SLEEP_S` seconds and refreshes the oldest bucket,
   /// unless they are all younger than 1 hour, in which case it goes back to sleep.
   ///
   /// This loop also republishes all entries each hour, provided we haven't received
   /// a `store` rpc for said entry in the past hour. It also clears expired entries.
   #[allow(unused_must_use)]
   fn maintenance_loop(resources: sync::Arc<resources::Resources>) {
      let hour = time::Duration::hours(1);
      let mut last_republish = time::SteadyTime::now();

      loop {
         thread::sleep(StdDuration::new(MAINTENANCE_SLEEP_S,0));
         if let State::ShuttingDown = *resources.state.read().unwrap() {
            break;
         }

         let now = time::SteadyTime::now();
         // If the oldest bucket was refreshed more than a hour ago,
         // or it was never refreshed, refresh it.
         match resources.table.oldest_bucket() {
            (i, None) => {resources.refresh_bucket(i);},
            (i, Some(time)) if (now - time) > hour => {resources.refresh_bucket(i);},
            _ => (),
         }
         
         resources.storage.clear_expired_entries();

         if now - last_republish > hour {
            for (key, entry, expiration) in resources.storage.get_all_ready_entries() {
               resources.store(key, entry, expiration);
            }

            last_republish = time::SteadyTime::now();
            resources.storage.mark_all_as_ready();
         }
      }
   }

   /// Initiates pings to stale nodes that have been part of an eviction
   /// conflict, and disposes of conflicts that haven't been resolved.
   #[allow(unused_must_use)]
   fn conflict_resolution_loop(resources: sync::Arc<resources::Resources>) {
      loop {
         let conflicts_empty = { // Lock scope
            let mut conflicts = resources.conflicts.lock().unwrap();
            // Conflicts that weren't solved in five pings are removed.
            // This means the incoming node that caused the conflict has priority.
            conflicts.retain(|&routing::EvictionConflict{times_pinged, ..}| times_pinged < 5);

            // We ping the evicted nodes for all conflicts that remain.
            for conflict in conflicts.iter_mut() {
               resources.ping_and_forget(&conflict.evicted);
               conflict.times_pinged += 1;
            }
            conflicts.is_empty()
         };

         // We wait for responses from these nodes.
         thread::sleep(StdDuration::new(1,0));
         
         let mut state = resources.state.write().unwrap();
         match *state {
            State::ShuttingDown => break,
            // If all conflicts are resolved, we leave defensive mode.
            State::Defensive if conflicts_empty => { 
               *state = if resources.table.len() > resources.configuration.k_factor { 
                     State::OnGrid 
                  } else {
                     State::OffGrid
                  };
                  
               resources.updates.lock().unwrap().broadcast(resources::Update::StateChange(*state));
            },
            _ => (),
         }
      }
   }
}

impl Drop for Node {
   fn drop(&mut self) {
      *self.resources.state.write().unwrap() = State::ShuttingDown;
      self.resources.updates.lock().unwrap().broadcast(resources::Update::StateChange(State::ShuttingDown));
   }
}
