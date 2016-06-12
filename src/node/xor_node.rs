use node::Node;
use node::hash::Sha1Hash;
use itertools::Zip;
use std::net::UdpSocket;
use std::io;

#[derive(Debug)]
pub struct XorNode {
   pub key : Sha1Hash,
   socket : UdpSocket,
}

impl XorNode {
   pub fn new() -> io::Result<XorNode> {
      let mut socket = try!(UdpSocket::bind("127.0.0.1:50000"));
      Ok(XorNode { key : Sha1Hash::new(), socket : socket })
   }
}

impl Node<Sha1Hash> for XorNode {
   fn distance(node_alpha : &Self, node_beta : &Self) -> Sha1Hash {
      let mut distance = Sha1Hash::new();
      for (d, a, b) in Zip::new((&mut distance.raw, &node_alpha.key.raw, &node_beta.key.raw)) {
         *d = a^b;
      }
      distance
   }
}

#[cfg(test)]
mod tests {
    use super::*;
    use node::hash::Sha1Hash;
    use node::Node;

    #[test]
    fn distance_between_two_new_nodes_is_zero() {
       let node_alpha = XorNode::new().unwrap(); 
       let node_beta = XorNode::new().unwrap(); 
       let distance = XorNode::distance(&node_alpha, &node_beta);

       for element in distance.raw.into_iter() {
         assert_eq!(*element,0);
       }
    }

    #[test]
    fn distance_between_two_nodes_is_XOR() {
       let mut node_alpha = XorNode::new().unwrap(); 
       let mut node_beta = XorNode::new().unwrap(); 

       node_alpha.key.raw[0] = 0xFF;
       node_alpha.key.raw[1] = 0xFF;
       node_beta.key.raw[1] = 0xFF;
       let distance = XorNode::distance(&node_alpha, &node_beta);
       assert_eq!(distance.raw[0], 0xFF);
       assert_eq!(distance.raw[1], 0x00);
    }
}
