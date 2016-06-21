use routing;
use rpc;

use hash::Hash;
use bincode::serde;
use std::{net, io, thread, time};
use std::sync::{ Weak, Arc};

pub const SOCKET_BUFFER_SIZE_BYTES : usize = 65536;
pub const SOCKET_TIMEOUT_S         : u64   = 5;

/// Subotai node. 
///
/// On construction, a detached thread for packet reception is
/// launched.
 pub struct Node {
   pub id     : Hash,
   table      : routing::Table,
   outbound   : net::UdpSocket,
   inbound    : net::UdpSocket,
}

impl Node {
   pub fn new(inbound_port: u16, outbound_port: u16) -> io::Result<Arc<Node>> {
      let id = Hash::random();

      let node = Arc::new(Node {
         id         : id.clone(),
         table      : routing::Table::new(id),
         inbound    : try!(net::UdpSocket::bind(("0.0.0.0", inbound_port))),
         outbound   : try!(net::UdpSocket::bind(("0.0.0.0", outbound_port))),
      });

      try!(node.inbound.set_read_timeout(Some(time::Duration::new(SOCKET_TIMEOUT_S,0))));

      let reception_node = Arc::downgrade(&node);
      thread::spawn(move || { Node::reception_loop(reception_node) });

      Ok(node)
   }

   /// Sends an RPC to a destination node. If the node address is known, the RPC will
   /// be sent directly. Otherwise, a node lookup will be performed first.
   pub fn send_rpc(&self, rpc: &rpc::Rpc, destination: &routing::NodeInfo)-> io::Result<()> {
      if let Some(address) = destination.address {
         let payload = rpc.serialize();
         try!(self.outbound.send_to(&payload, address));
      }
      else {  
         // Perform lookup.
         unimplemented!();  
      }
      Ok(())
   }

   /// Receives and processes data as long as the table is alive. Will gracefully exit, at most,
   /// `SOCKET_TIMEOUT_S` seconds after the table is dropped.
   fn reception_loop(node: Weak<Node>) {
      let mut buffer = [0u8; SOCKET_BUFFER_SIZE_BYTES];

      while let Some(node) = node.upgrade() {
         if let Ok((_, source)) = node.inbound.recv_from(&mut buffer) {
            node.table.process_incoming_rpc(&buffer, source);
         }
      }
   }
}

impl routing::Table {
   fn process_incoming_rpc(&self, buffer: &[u8], mut source: net::SocketAddr) -> serde::DeserializeResult<()> {
      let rpc = try!(rpc::Rpc::deserialize(buffer));

      source.set_port(rpc.reply_port);
      let sender_node = routing::NodeInfo {
         node_id : rpc.sender_id,
         address : Some(source),
      };

      match rpc.kind {
         rpc::Kind::Ping => self.insert_node(sender_node),
         _ => (),
      }
      Ok(())
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
         node_id : beta.id.clone(),
         address : Some(address_beta),
      };
      let ping = rpc::Rpc::ping(alpha.id.clone(), 50000);

      // Before sending the ping, beta does not know about alpha.
      assert!(beta.table.specific_node(&alpha.id).is_none());

      // Alpha pings beta.
      alpha.send_rpc(&ping, &info_beta);

      // Eventually, beta knows of alpha.
      let mut found = beta.table.specific_node(&alpha.id);
      for _ in 0..TRIES {
         if found.is_some() {
            break;
         }
         found = beta.table.specific_node(&alpha.id);
         thread::sleep(time::Duration::from_millis(POLL_FREQUENCY_MS));
      }

      assert!(found.is_some());
   }
}
