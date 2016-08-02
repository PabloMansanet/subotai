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

use {routing, rpc, bus, SubotaiResult};
use hash::Hash;
use std::{net, thread, sync};
use std::time::Duration as StdDuration;

/// Timeout period in seconds to stop waiting for a remote node response. 
pub const NETWORK_TIMEOUT_S : i64 = 5;

/// Size of a typical UDP socket buffer.
pub const SOCKET_BUFFER_SIZE_BYTES : usize = 65536;

const SOCKET_TIMEOUT_MS     : u64   = 200;
const UPDATE_BUS_SIZE_BYTES : usize = 50;

/// Subotai node. 
///
/// On construction, it launches a detached thread for packet reception.
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
   pub fn id(&self) -> &Hash {
      &self.resources.id
   }

   /// Returns the current state of the node.
   pub fn state(&self)-> State {
      *self.resources.state.lock().unwrap()
   }

   /// Constructs a node with a given inbound/outbound UDP port pair.
   pub fn with_ports(inbound_port: u16, outbound_port: u16) -> SubotaiResult<Node> {
      let id = Hash::random();

      let resources = sync::Arc::new(resources::Resources {
         id         : id.clone(),
         table      : routing::Table::new(id),
         inbound    : try!(net::UdpSocket::bind(("0.0.0.0", inbound_port))),
         outbound   : try!(net::UdpSocket::bind(("0.0.0.0", outbound_port))),
         state      : sync::Mutex::new(State::OffGrid),
         updates    : sync::Mutex::new(bus::Bus::new(UPDATE_BUS_SIZE_BYTES)),
         conflicts  : sync::Mutex::new(Vec::with_capacity(routing::MAX_CONFLICTS)),
      });

      try!(resources.inbound.set_read_timeout(Some(StdDuration::from_millis(SOCKET_TIMEOUT_MS))));

      let reception_resources = resources.clone();
      thread::spawn(move || { Node::reception_loop(reception_resources) });

      let conflict_resolution_resources = resources.clone();
      thread::spawn(move || { Node::conflict_resolution_loop(conflict_resolution_resources) });

      Ok( Node{ resources: resources } )
   }


   /// Produces an iterator over RPCs received by this node. The iterator will block
   /// indefinitely.
   pub fn receptions(&self) -> receptions::Receptions {
      self.resources.receptions()
   }

   /// Sends a ping RPC to a destination node. If the ID is unknown, this request is 
   /// promoted into a find_node RPC followed by a ping to the node.
   pub fn ping(&self, id: &Hash) -> SubotaiResult<()> {
      self.resources.ping(id)
   }

   /// Produces an ID-Address pair, with the node's local inbound UDPv4 address.
   pub fn local_info(&self) -> NodeInfo {
      self.resources.local_info()
   }

   /// Bootstraps the node from a seed, and returns the amount of nodes in the final table.
   pub fn bootstrap(&self, seed: NodeInfo) -> SubotaiResult<usize> {
       try!(self.resources.bootstrap(seed, None));
       *self.resources.state.lock().unwrap() = State::Alive;
       Ok(self.resources.table.len())
   }

   /// Bootstraps to a network with a limited number of nodes.
   pub fn bootstrap_until(&self, seed: NodeInfo, network_size: usize) -> SubotaiResult<usize> {
       try!(self.resources.bootstrap(seed, Some(network_size)));
       *self.resources.state.lock().unwrap() = State::Alive;
       Ok(self.resources.table.len())
   }

   /// Recursive node lookup through the network. Will block until
   /// finished and return the node information if succeful.
   pub fn find_node(&self, id: &Hash) -> SubotaiResult<NodeInfo> {
      self.resources.find_node(id)
   }

   /// Receives and processes data as long as the table is alive.
   fn reception_loop(resources: sync::Arc<resources::Resources>) {
      let mut buffer = [0u8; SOCKET_BUFFER_SIZE_BYTES];

      loop {
         let message = resources.inbound.recv_from(&mut buffer);
         if let State::ShuttingDown = *resources.state.lock().unwrap() {
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

   /// Initiates pings to stale nodes that have been part of an eviction
   /// conflict, and disposes of conflicts that haven't been resolved.
   #[allow(unused_must_use)]
   fn conflict_resolution_loop(resources: sync::Arc<resources::Resources>) {
      loop {
         if let State::ShuttingDown = *resources.state.lock().unwrap() {
            break;
         }
         
         { // Lock scope
            let mut conflicts = resources.conflicts.lock().unwrap();
            // Conflicts that weren't solved in five pings are removed.
            // This means the incoming node that caused the conflict has priority.
            conflicts.retain(|&routing::EvictionConflict{times_pinged, ..}| times_pinged < 5);
            // We ping the evicted nodes for all conflicts that remain.
            for conflict in conflicts.iter_mut() {
               resources.ping_for_conflict(&conflict.evicted);
               conflict.times_pinged += 1;
            }
         }
         // We wait for responses from these nodes.
         thread::sleep(StdDuration::new(1u64,0));
      }
   }
}

impl Drop for Node {
   fn drop(&mut self) {
      *self.resources.state.lock().unwrap() = State::ShuttingDown;
   }
}

