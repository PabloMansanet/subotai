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
//! at which point the state will change to `Alive`.
//!
//! Destroying a node automatically schedules all threads to terminate after finishing 
//! any pending operations.

pub mod receptions;
pub use routing::NodeInfo as NodeInfo;

#[cfg(test)]
mod tests;
mod resources;

use {storage, routing, rpc, bus, SubotaiResult, time};
use hash::SubotaiHash;
use std::{net, thread, sync};
use std::time::Duration as StdDuration;

/// Timeout period in seconds to stop waiting for a remote node response. 
pub const NETWORK_TIMEOUT_S : i64 = 5;

/// Size of a typical UDP socket buffer.
pub const SOCKET_BUFFER_SIZE_BYTES : usize = 65536;

const SOCKET_TIMEOUT_MS     : u64   = 200;
const UPDATE_BUS_SIZE_BYTES : usize = 50;

/// Maintenance thread sleep period.
const MAINTENANCE_SLEEP_S : u64 = 5;

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
   Alive,
   /// The node is in defensive mode. Too many conflicts have 
   /// been generated recently, so the node gives preference
   /// to its older contacts until all conflicts are resolved.
   Defensive,
   /// The node is in an error state.
   Error,
   /// The node is in a process of shutting down;
   /// all of its resources will be deallocated after completion
   /// of any pending async operations.
   ShuttingDown,
}

impl Node {
   /// Constructs a node with OS allocated random ports.
   pub fn new() -> SubotaiResult<Node> {
      Node::with_ports(0, 0)
   }

   /// Returns the hash used to identify this node in the network.
   pub fn id(&self) -> &SubotaiHash {
      &self.resources.id
   }

   /// Returns the current state of the node.
   pub fn state(&self)-> State {
      *self.resources.state.read().unwrap()
   }

   /// Constructs a node with a given inbound/outbound UDP port pair.
   pub fn with_ports(inbound_port: u16, outbound_port: u16) -> SubotaiResult<Node> {
      let id = SubotaiHash::random();
      
      let resources = sync::Arc::new(resources::Resources {
         id        : id.clone(),
         table     : routing::Table::new(id),
         storage   : storage::Storage::new(),
         inbound   : try!(net::UdpSocket::bind(("0.0.0.0", inbound_port))),
         outbound  : try!(net::UdpSocket::bind(("0.0.0.0", outbound_port))),
         state     : sync::RwLock::new(State::OffGrid),
         updates   : sync::Mutex::new(bus::Bus::new(UPDATE_BUS_SIZE_BYTES)),
         conflicts : sync::Mutex::new(Vec::with_capacity(routing::MAX_CONFLICTS)),
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


   /// Produces an iterator over RPCs received by this node. The iterator will block
   /// indefinitely.
   pub fn receptions(&self) -> receptions::Receptions {
      self.resources.receptions()
   }

   /// Bootstraps the node from a seed, and returns the amount of nodes in the final table.
   pub fn bootstrap(&self, seed: NodeInfo) -> SubotaiResult<usize> {
       self.resources.table.update_node(seed);
       try!(self.resources.probe(&self.resources.id, routing::K_FACTOR));
       *self.resources.state.write().unwrap() = State::Alive;
       Ok(self.resources.table.len())
   }

   /// Bootstraps to a network with a limited number of nodes.
   pub fn bootstrap_until(&self, seed: NodeInfo, network_size: usize) -> SubotaiResult<usize> {
       self.resources.table.update_node(seed);
       try!(self.resources.probe(&self.resources.id, network_size));
       *self.resources.state.write().unwrap() = State::Alive;
       Ok(self.resources.table.len())
   }

   pub fn local_info(&self) -> NodeInfo {
      self.resources.local_info()
   }

   /// Stores a key-value pair in the network.
   pub fn store(&self, key: &SubotaiHash, value: &SubotaiHash) -> SubotaiResult<()> {
      let storage_candidates = try!(self.resources.probe(&key, routing::K_FACTOR));
      for candidate in &storage_candidates {
         try!(self.resources.store_remotely(candidate, key.clone(), value.clone()));
      }
      Ok(())
   }

   /// Retrieves a value from the network, given a key.
   pub fn retrieve(&self, key: &SubotaiHash) -> SubotaiResult<SubotaiHash> {
      self.resources.retrieve(key)
   }

   /// Receives and processes data as long as the node is alive.
   fn reception_loop(resources: sync::Arc<resources::Resources>) {
      let mut buffer = [0u8; SOCKET_BUFFER_SIZE_BYTES];

      loop {
         let message = resources.inbound.recv_from(&mut buffer);
         if let State::ShuttingDown = *resources.state.read().unwrap() {
            resources.updates.lock().unwrap().broadcast(resources::Update::Shutdown);
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
   #[allow(unused_must_use)]
   fn maintenance_loop(resources: sync::Arc<resources::Resources>) {
      let hour = time::Duration::hours(1);
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
               resources.ping_for_conflict(&conflict.evicted);
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
            State::Defensive if conflicts_empty => *state = State::Alive,
            _ => (),
         }
      }
   }

}

impl Drop for Node {
   fn drop(&mut self) {
      *self.resources.state.write().unwrap() = State::ShuttingDown;
   }
}
