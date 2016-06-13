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
   pub fn new(port: u16) -> io::Result<XorNode> {
      let socket = try!(UdpSocket::bind(("0.0.0.0", port)));
      Ok(XorNode { key : Sha1Hash::from_string(""), socket : socket })
   }
}

impl Node<Sha1Hash, Sha1Hash> for XorNode {
   fn distance(node_alpha : &Self, node_beta : &Self) -> Sha1Hash {
      let mut distance = Sha1Hash::from_string("");
      for (d, a, b) in Zip::new((&mut distance.raw, &node_alpha.key.raw, &node_beta.key.raw)) {
         *d = a^b;
      }
      distance
   }
}

#[cfg(test)]
mod tests {
    use super::*;
    use node::Node;
    use itertools::Zip;

    #[test]
    fn distance_between_two_new_nodes_is_zero() {
       let node_alpha = XorNode::new(50000).unwrap(); 
       let node_beta = XorNode::new(50001).unwrap(); 
       let distance = XorNode::distance(&node_alpha, &node_beta);

       for element in distance.raw.into_iter() {
         assert_eq!(*element,0);
       }
    }

    #[test]
    fn distance_between_two_nodes_is_xor() {
       let node_alpha = XorNode::new(50000).unwrap(); 
       let node_beta = XorNode::new(50001).unwrap(); 

       let distance = XorNode::distance(&node_alpha, &node_beta);
       for (d, a, b) in Zip::new((&distance.raw, &node_alpha.key.raw, &node_beta.key.raw)) {
          assert_eq!(*d, *a^*b);
       }
    }
}
