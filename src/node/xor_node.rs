use node::Node;
use node::hash::BinHash;


#[derive(Debug)]
pub struct XorNode {
   key : BinHash,
}

impl XorNode {
   pub fn new() -> XorNode {
      XorNode { key : BinHash::new() }
   }
}

impl Node<BinHash> for XorNode {
   fn distance(node_alpha : &Self, node_beta : &Self) -> BinHash {
      BinHash::new()
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
       let distance = Node<BinHash>::distance(&node_alpha, &node_beta);
    }
}
