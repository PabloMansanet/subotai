use hash::Hash;
use routing;
use rpc;
use std::net;
use std::io;
use std::thread;
use std::sync::Weak;
use std::sync::Arc;
use std::time::Duration;

pub const SOCKET_BUFFER_SIZE_BYTES : usize = 65536;
pub const SOCKET_TIMEOUT_S         : u64   = 5;

pub struct Node {
   pub id    : Hash,
   table     : Arc<routing::Table>,
   outbound  : net::UdpSocket,
}

impl Node {
   /// Constructs a node and launches a reception thread, that will take care of processing
   /// RPCs from other nodes asynchronously.
   pub fn new(inbound_port: u16, outbound_port: u16) -> io::Result<Node> {
      let id = Hash::random();
      let table = Arc::new(routing::Table::new(id.clone()));
      let table_weak = Arc::downgrade(&table);
      let inbound = try!(net::UdpSocket::bind(("0.0.0.0", inbound_port)));
      let outbound = try!(net::UdpSocket::bind(("0.0.0.0", outbound_port)));
      try!(inbound.set_read_timeout(Some(Duration::new(SOCKET_TIMEOUT_S,0))));

      thread::spawn(move || { Node::reception_loop(table_weak, inbound); });

      Ok(Node {id: id, table: table, outbound: outbound})
   }

   pub fn send_rpc(&self, rpc: &rpc::Rpc, destination: &routing::NodeInfo) {
      if let Some(address) = destination.address {
         let payload = rpc.serialize();
         self.outbound.send_to(&payload, address);
      }
      else {  
         // Perform lookup.
         unimplemented!();  
      }
   }

   /// Receives and processes data as long as the table is alive. Will gracefully exit, at most,
   /// `SOCKET_TIMEOUT_S` seconds after the table is dropped.
   fn reception_loop(table_weak: Weak<routing::Table>, socket: net::UdpSocket) {
      let mut buffer = [0u8; SOCKET_BUFFER_SIZE_BYTES];

      loop {
         if let Ok((bytes, source)) = socket.recv_from(&mut buffer) {
            if let Some(table) = table_weak.upgrade() {
               table.process_incoming_rpc(&buffer, bytes, source);
            }
         }

         if table_weak.upgrade().is_none() {
            break;
         }
      }
   }
}

impl routing::Table {
   fn process_incoming_rpc(&self, buffer: &[u8], bytes: usize, source: net::SocketAddr) {
   }
}

#[cfg(test)]
mod tests {
   use node;
   use rpc;
   use routing;
   use std::net;
   use std::str::FromStr;

   #[test]
   fn node_ping() {
      let alpha = node::Node::new(50000, 50001).unwrap();
      let beta  = node::Node::new(50002, 50003).unwrap();

      let ip = net::IpAddr::from_str("127.0.0.1").unwrap();
      let address_beta = net::SocketAddr::new(ip, 50002);

      let info_beta = routing::NodeInfo { 
         node_id : beta.id,
         address : Some(address_beta),
      };
      let ping = rpc::Rpc::ping();

      alpha.send_rpc(&ping, &info_beta);
   }
}
