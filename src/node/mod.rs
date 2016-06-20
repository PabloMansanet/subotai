use hash::Hash;
use std::net::UdpSocket;
use std::io;

#[derive(Debug)]
pub struct Node {
   pub key : Hash,
   socket : UdpSocket,
}

impl Node {
   pub fn new(port: u16) -> io::Result<Node> {
      let socket = try!(UdpSocket::bind(("0.0.0.0", port)));
      Ok(Node { key : Hash::random(), socket : socket })
   }

   fn distance(node_alpha : &Self, node_beta : &Self) -> Hash {
      &node_alpha.key ^ &node_beta.key
   }
}

