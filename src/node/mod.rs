use hash::Hash;
use routing;
use std::net;
use std::io;
use std::thread;
use std::sync::Arc;
use std::time::Duration;

pub const SOCKET_BUFFER_SIZE_BYTES : usize = 65536;
pub const SOCKET_TIMEOUT_S         : u64   = 5;

pub struct Node {
   pub id    : Hash,
   table     : Arc<routing::Table>,
   reception : thread::JoinHandle<()>,
}

impl Node {
   /// Constructs a node and launches a reception thread, that will take care of processing
   /// RPCs from other nodes asynchronously.
   pub fn new(port: u16) -> io::Result<Node> {

      let id = Hash::random();
      let table = Arc::new(routing::Table::new(id.clone()));
      let table_weak = Arc::downgrade(&table);
      let socket = try!(net::UdpSocket::bind(("0.0.0.0", port)));
      socket.set_read_timeout(Some(Duration::new(SOCKET_TIMEOUT_S,0)));

      let reception = thread::spawn(move || {
         let mut buffer = [0u8; SOCKET_BUFFER_SIZE_BYTES];

         loop {
            match socket.recv_from(&mut buffer) {
               Ok((bytes, source)) => {
                  match table_weak.upgrade() {
                     Some(table) => table.process_incoming_rpc(&buffer, bytes, source),
                     None => break,
                  }
               }
               Err(_) => {
                  if table_weak.upgrade().is_none() {
                     break;
                  }
               }
            }
         }
      });

      Ok( Node { 
         id        : id,
         table     : table,
         reception : reception
      })
   }
}

impl routing::Table {
   fn process_incoming_rpc(&self, buffer: &[u8], bytes: usize, source: net::SocketAddr) {
   }
}

#[cfg(test)]
mod tests {
    #[test]
    fn concurrent_access_to_table() {
    }
}
