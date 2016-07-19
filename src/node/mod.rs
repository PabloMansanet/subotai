pub mod receptions;

#[cfg(test)]
mod tests;
mod resources;

use routing;
use rpc;
use bus;

pub use routing::NodeInfo as NodeInfo;

use hash::Hash;
use std::{net, io, thread};
use std::sync;
use std::time::Duration as StdDuration;
use std::sync::Arc;

/// Timeout period in seconds to stop waiting for a remote node response. 
pub const NETWORK_TIMEOUT_S : i64 = 5;

/// Size of a typical UDP socket buffer.
pub const SOCKET_BUFFER_SIZE_BYTES : usize = 65536;

const SOCKET_TIMEOUT_S         : u64   = 1;
const UPDATE_BUS_SIZE_BYTES    : usize = 50;

/// Subotai node. 
///
/// On construction, a detached thread for packet reception is
/// launched. 
pub struct Node {
   resources: Arc<resources::Resources>,
}

/// State of a Subotai node. 
///
/// * `OffGrid`: The node is initialized but disconnected from the 
///   network. Needs to go through succesful bootstrapping.
///
/// * `Alive`: The node is online and connected to the network.
///
/// * `Error`: The node is in an error state.
///
/// * `ShuttingDown`: The node is in a process of shutting down;
///   all of it's resources will be deallocated after completion
///   of any pending async operations.
#[derive(Debug, Eq, PartialEq)]
pub enum State {
   OffGrid,
   Alive,
   Error,
   ShuttingDown,
}

/// Result of a ping operation
/// * `Alive`:
#[derive(Debug, Eq, PartialEq)]
pub enum PingResult {
   Alive,
   NoResponse,
}

impl Node {
   /// Constructs a node with OS allocated random ports.
   pub fn new() -> Node {
      Node::with_ports(0,0).unwrap()
   }

   /// Returns the randomly generated hash used to identify this node in the network.
   pub fn id<'a>(&'a self) -> &Hash {
      &self.resources.id
   }

   /// Constructs a node with a given inbound/outbound UDP port pair.
   pub fn with_ports(inbound_port: u16, outbound_port: u16) -> io::Result<Node> {
      let id = Hash::random();

      let resources = Arc::new(resources::Resources {
         id         : id.clone(),
         table      : routing::Table::new(id),
         inbound    : try!(net::UdpSocket::bind(("0.0.0.0", inbound_port))),
         outbound   : try!(net::UdpSocket::bind(("0.0.0.0", outbound_port))),
         state      : sync::Mutex::new(State::OffGrid),
         updates    : sync::Mutex::new(bus::Bus::new(UPDATE_BUS_SIZE_BYTES))
      });

      try!(resources.inbound.set_read_timeout(Some(StdDuration::new(SOCKET_TIMEOUT_S,0))));

      let reception_resources = resources.clone();
      thread::spawn(move || { Node::reception_loop(reception_resources) });

      Ok( Node{ resources: resources } )
   }


   /// Produces an iterator over RPCs received by this node. The iterator will block
   /// indefinitely.
   pub fn receptions(&self) -> receptions::Receptions {
      self.resources.receptions()
   }

   /// Sends a ping RPC to a destination node. If the ID is unknown, this request is 
   /// promoted into a find_node RPC followed by a ping to the node. Returns the
   /// node information if the node responded, and None in case of timeout.
   pub fn ping(&self, id: Hash) -> PingResult {
      self.resources.ping(id)
   }

   /// Produces an ID-Address pair, with the node's local inbound UDPv4 address.
   pub fn local_info(&self) -> NodeInfo {
      self.resources.local_info()
   }

   pub fn bootstrap(&self, seed: NodeInfo) {
       self.resources.bootstrap(seed);
       *self.resources.state.lock().unwrap() = State::Alive;
   }

   /// Recursive node lookup through the network. Will block until
   /// finished and return the node information if succeful.
   pub fn find_node(&self, id: &Hash) -> Option<NodeInfo> {
      self.resources.find_node(id)
   }

   /// Receives and processes data as long as the table is alive.
   fn reception_loop(resources: Arc<resources::Resources>) {
      let mut buffer = [0u8; SOCKET_BUFFER_SIZE_BYTES];

      loop {
         if let State::ShuttingDown = *resources.state.lock().unwrap() {
            break;
         }

         if let Ok((_, source)) = resources.inbound.recv_from(&mut buffer) {
            if let Ok(rpc) = rpc::Rpc::deserialize(&buffer) {
               let resources_clone = resources.clone();
               thread::spawn(move || { resources_clone.process_incoming_rpc(rpc, source) } );
            }
         }

         resources.updates.lock().unwrap().broadcast(resources::Update::Tick);
      }
   }
}

impl Drop for Node {
   fn drop(&mut self) {
      *self.resources.state.lock().unwrap() = State::ShuttingDown;
   }
}

