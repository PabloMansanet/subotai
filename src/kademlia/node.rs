use node;
use hash::Sha1Hash;
use itertools::Zip;
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
      let mut distance = Sha1Hash::from_string("");
      for (d, a, b) in Zip::new((&mut distance.raw, &node_alpha.key.raw, &node_beta.key.raw)) {
         *d = a^b;
      }
      distance
   }

   fn ping(&self, destination : &Sha1Hash) {
   }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Zip;
    use node::Node as NodeTrait;

    #[test]
    fn distance_between_two_new_nodes_is_zero() {
       let node_alpha = Node::new(50000).unwrap(); 
       let node_beta = Node::new(50001).unwrap(); 
       let distance = Node::distance(&node_alpha, &node_beta);

       for element in distance.raw.into_iter() {
         assert_eq!(*element,0);
       }
    }

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
