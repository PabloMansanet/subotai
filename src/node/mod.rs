#[cfg(test)]
mod tests;
mod resources;

use routing;
use rpc;
use bus;

use hash::Hash;
use bincode::serde;
use std::{net, io, thread, time};
use std::sync;
use std::sync::{Weak, Arc};

pub const SOCKET_BUFFER_SIZE_BYTES : usize = 65536;
pub const SOCKET_TIMEOUT_S         : u64   = 5;
pub const UPDATE_BUS_SIZE_BYTES    : usize = 50;

/// Subotai node. 
///
/// On construction, a detached thread for packet reception is
/// launched. 
pub struct Node {
   resources: Arc<resources::Resources>,
}

#[derive(Eq, PartialEq)]
pub enum State {
   Alive,
   Error,
   ShuttingDown,
}

/// A blocking iterator over the RPCs this node receives. 
pub struct Receptions {
   iter: bus::BusIntoIter<rpc::Rpc>,
}

impl Node {
   pub fn new(inbound_port: u16, outbound_port: u16) -> io::Result<Node> {
      let id = Hash::random();

      let resources = Arc::new(resources::Resources {
         id         : id.clone(),
         table      : routing::Table::new(id),
         inbound    : try!(net::UdpSocket::bind(("0.0.0.0", inbound_port))),
         outbound   : try!(net::UdpSocket::bind(("0.0.0.0", outbound_port))),
         state      : sync::Mutex::new(State::Alive),
         received   : sync::Mutex::new(bus::Bus::new(UPDATE_BUS_SIZE_BYTES))
      });

      try!(resources.inbound.set_read_timeout(Some(time::Duration::new(SOCKET_TIMEOUT_S,0))));

      let weak_resources = Arc::downgrade(&resources);
      thread::spawn(move || { Node::reception_loop(weak_resources) });

      Ok( Node{ resources: resources } )
   }

   /// Produces an iterator over RPCs received by this node. The iterator will block
   /// indefinitely.
   pub fn receptions(&self) -> Receptions {
      Receptions { iter: self.resources.received.lock().unwrap().add_rx().into_iter() }
   }

   /// Sends a ping RPC to a destination node.
   pub fn ping(&self, destination: routing::NodeInfo) {
      let resources = self.resources.clone();
      thread::spawn(move || { resources.ping(destination) });
   }

   /// Receives and processes data as long as the table is alive.
   fn reception_loop(weak: Weak<resources::Resources>) {
      let mut buffer = [0u8; SOCKET_BUFFER_SIZE_BYTES];

      while let Some(strong) = weak.upgrade() {
         if let State::ShuttingDown = *strong.state.lock().unwrap() {
            break;
         }

         if let Ok((_, source)) = strong.inbound.recv_from(&mut buffer) {
            if let Ok(rpc) = rpc::Rpc::deserialize(&buffer) {
               thread::spawn(move || { strong.process_incoming_rpc(rpc, source) } );
            }
         }
      }
   }
}

impl Drop for Node {
   fn drop(&mut self) {
      *self.resources.state.lock().unwrap() = State::ShuttingDown;
   }
}

impl Iterator for Receptions {
   type Item = rpc::Rpc;

   fn next(&mut self) -> Option<rpc::Rpc> {
      self.iter.next()
   }
}

