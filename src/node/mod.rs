use routing;
use rpc;

use hash::Hash;
use bincode::serde;
use std::{net, io, thread, time};
use std::sync;
use std::sync::{Weak, Arc};

pub const SOCKET_BUFFER_SIZE_BYTES : usize = 65536;
pub const SOCKET_TIMEOUT_S         : u64   = 5;

/// Subotai node. 
///
/// On construction, a detached thread for packet reception is
/// launched.
pub struct Node {
   resources: Arc<Resources>,
}

struct Resources {
   pub id   : Hash,
   table    : routing::Table,
   outbound : net::UdpSocket,
   inbound  : net::UdpSocket,
   state    : sync::Mutex<State>,
}

#[derive(Eq, PartialEq)]
enum State {
   Alive,
   Error,
   ShuttingDown,
}

impl Node {
   pub fn new(inbound_port: u16, outbound_port: u16) -> io::Result<Node> {
      let id = Hash::random();

      let resources = Arc::new(Resources {
         id         : id.clone(),
         table      : routing::Table::new(id),
         inbound    : try!(net::UdpSocket::bind(("0.0.0.0", inbound_port))),
         outbound   : try!(net::UdpSocket::bind(("0.0.0.0", outbound_port))),
         state      : sync::Mutex::new(State::Alive),
      });

      try!(resources.inbound.set_read_timeout(Some(time::Duration::new(SOCKET_TIMEOUT_S,0))));

      let weak_resources = Arc::downgrade(&resources);
      thread::spawn(move || { Node::reception_loop(weak_resources) });

      Ok( Node{ resources: resources } )
   }

   pub fn ping(&self, destination: routing::NodeInfo) {
      let resources = self.resources.clone();
      thread::spawn(move || {
         if let Some(address) = destination.address {
            let ping = rpc::Rpc::ping(resources.id.clone(), resources.inbound.local_addr().unwrap().port());
            let payload = ping.serialize();
            resources.outbound.send_to(&payload, address);
         }
      });
   }


   /// Receives and processes data as long as the table is alive.
   fn reception_loop(weak: Weak<Resources>) {
      let mut buffer = [0u8; SOCKET_BUFFER_SIZE_BYTES];

      while let Some(strong) = weak.upgrade() {
         if let State::ShuttingDown = *strong.state.lock().unwrap() {
            break;
         }

         if let Ok((_, source)) = strong.inbound.recv_from(&mut buffer) {
            strong.process_incoming_rpc(&buffer, source);
         }
      }
   }
}

impl Drop for Node {
   fn drop(&mut self) {
      *self.resources.state.lock().unwrap() = State::ShuttingDown;
   }
}

impl Resources {
   fn process_incoming_rpc(&self, buffer: &[u8], mut source: net::SocketAddr) -> serde::DeserializeResult<()> {
      let rpc = try!(rpc::Rpc::deserialize(buffer));

      source.set_port(rpc.reply_port);
      let sender = routing::NodeInfo {
         node_id : rpc.sender_id.clone(),
         address : Some(source),
      };

      match rpc.kind {
         rpc::Kind::Ping => self.handle_ping(rpc, sender),
         _ => (),
      }
      Ok(())
   }

   fn handle_ping(&self, ping: rpc::Rpc, sender: routing::NodeInfo){
      self.table.insert_node(sender);
      // TODO send ping response
   }
}

#[cfg(test)]
mod tests {
   use node;
   use rpc;
   use std::thread;
   use std::time;
   use routing;
   use std::net;
   use std::str::FromStr;
   pub const POLL_FREQUENCY_MS: u64 = 50;
   pub const TRIES: u8 = 5;

   #[test]
   fn node_ping() {
      let alpha = node::Node::new(50000, 50001).unwrap();
      let beta  = node::Node::new(50002, 50003).unwrap();

      let ip = net::IpAddr::from_str("127.0.0.1").unwrap();
      let address_beta = net::SocketAddr::new(ip, 50002);

      let info_beta = routing::NodeInfo { 
         node_id : beta.resources.id.clone(),
         address : Some(address_beta),
      };

      // Before sending the ping, beta does not know about alpha.
      assert!(beta.resources.table.specific_node(&alpha.resources.id).is_none());

      // Alpha pings beta.
      alpha.ping(info_beta);

      // Eventually, beta knows of alpha.
      let mut found = beta.resources.table.specific_node(&alpha.resources.id);
      for _ in 0..TRIES {
         if found.is_some() {
            break;
         }
         found = beta.resources.table.specific_node(&alpha.resources.id);
         thread::sleep(time::Duration::from_millis(POLL_FREQUENCY_MS));
      }

      assert!(found.is_some());
   }
}
