use node::Node;
use node::hash::BinHash;


#[derive(Debug)]
pub struct XorNode {
   pub key : BinHash,
}

impl XorNode {
   pub fn new() -> XorNode {
      XorNode { key : BinHash::new() }
   }
}

impl Node<BinHash> for XorNode {
   fn distance(node_alpha : &Self, node_beta : &Self) -> BinHash {
   }
}

#[cfg(test)]
mod tests {
    use super::*;
    use node::hash::BinHash;
    use node::Node;

    #[test]
    fn distance_between_two_new_nodes_is_zero() {
       let node_alpha = XorNode::new(); 
       let node_beta = XorNode::new(); 
       let distance = XorNode::distance(&node_alpha, &node_beta);

       for element in distance.raw.into_iter() {
         assert_eq!(*element,0);
       }
    }

    #[test]
    fn distance_between_two_nodes_is_XOR() {
       let mut node_alpha = XorNode::new(); 
       let node_beta = XorNode::new(); 

       node_alpha.key.raw[0] = 0xFF;
       let distance = XorNode::distance(&node_alpha, &node_beta);
       assert_eq!(distance.raw[0], 0xFF);
    }
}
