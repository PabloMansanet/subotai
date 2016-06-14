use node;
use hash::Hash160;
use std::net::UdpSocket;
use std::io;

#[derive(Debug)]
pub struct Node {
   pub key : Hash160,
   socket : UdpSocket,
}

impl Node {
   pub fn new(port: u16) -> io::Result<Node> {
      let socket = try!(UdpSocket::bind(("0.0.0.0", port)));
      Ok(Node { key : Hash160::random(), socket : socket })
   }
}

impl node::Node<Hash160, Hash160> for Node {
   fn distance(node_alpha : &Self, node_beta : &Self) -> Hash160 {
      &node_alpha.key ^ &node_beta.key
   }
}
