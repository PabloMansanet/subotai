use node;
use hash::Sha1Hash;
use std::net::UdpSocket;
use std::io;

#[derive(Debug)]
pub struct Node {
   pub key : Sha1Hash,
   socket : UdpSocket,
}

impl Node {
   pub fn new(port: u16) -> io::Result<Node> {
      let socket = try!(UdpSocket::bind(("0.0.0.0", port)));
      Ok(Node { key : Sha1Hash::from_string(""), socket : socket })
   }
}

impl node::Node<Sha1Hash, Sha1Hash> for Node {
   fn distance(node_alpha : &Self, node_beta : &Self) -> Sha1Hash {
      Sha1Hash::xor_distance(&node_alpha.key, &node_beta.key)
   }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Zip;
    use node::Node as NodeTrait;

    #[test]
    fn distance_between_two_nodes_is_xor() {
       let node_alpha = Node::new(50000).unwrap(); 
       let node_beta = Node::new(50001).unwrap(); 

       let distance = Node::distance(&node_alpha, &node_beta);
       for (d, a, b) in Zip::new((&distance.raw, &node_alpha.key.raw, &node_beta.key.raw)) {
          assert_eq!(*d, *a^*b);
       }
    }
}
