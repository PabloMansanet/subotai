use hash::KEY_SIZE;
use hash::Hash160;
use std::net;
use std::collections::VecDeque;

const BUCKET_DEPTH : usize = 20;

pub struct RoutingTable {
   parent_key : Hash160,
   buckets : Vec<Bucket>
}

struct Bucket {
   entries  : VecDeque<NodeInfo>
}

#[derive(Clone)]
pub struct NodeInfo {
   key  : Hash160,
   ip   : net::IpAddr,
   port : u16,
}

impl RoutingTable {
   pub fn new(parent_key : &Hash160) -> RoutingTable {
      let mut buckets = Vec::<Bucket>::with_capacity(KEY_SIZE);
      for _ in 0..KEY_SIZE {
         buckets.push(Bucket::new());
      }

      RoutingTable { parent_key : parent_key.clone(), buckets : buckets }
   }

   /// Returns a table entry for the specific node with a given hash.
   fn specific_node(&self, key : &Hash160) -> Option<NodeInfo> {
      if let Some(index) = self.bucket_for_node(key){
         let ref bucket = self.buckets[index];
         return bucket.entries.iter().find(|ref info| *key == info.key).map(|x| x.clone());
      }
      None
   }

   /// Returns the appropriate position for a node, by computing
   /// the last bit of their distance set to 1. None if we are
   /// attempting to add the parent key.
   fn bucket_for_node(&self, key : &Hash160) -> Option<usize> {
       Hash160::xor_distance(&self.parent_key, &key).height()
   }

}

impl Bucket {
   pub fn new() -> Bucket {
      Bucket { entries : VecDeque::<NodeInfo>::with_capacity(BUCKET_DEPTH) }
   }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hash::Hash160;

    #[test]
    fn new_routing_table_is_empty() {
       let parent_key = Hash160::random();
       let table = RoutingTable::new(&parent_key);
       let test_hash = Hash160::random();
       
       assert!(table.specific_node(&test_hash).is_none());
    }
}
