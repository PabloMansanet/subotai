use hash::KEY_SIZE;
use hash::Hash160;
use std::net;
use std::collections::VecDeque;

const BUCKET_DEPTH : usize = 20;

/// Kademlia routing table, with 160 buckets of BUCKET_DEPTH (k) node
/// identifiers each.
pub struct RoutingTable {
   parent_key : Hash160,
   buckets : Vec<Bucket>
}

/// Bucket size is estimated to be small enough not to warrant
/// the downsides of using a linked list.
struct Bucket {
   entries  : VecDeque<NodeInfo>
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeInfo {
   key  : Hash160,
   ip   : net::IpAddr,
   port : u16,
}

impl RoutingTable {
   pub fn new(parent_key : Hash160) -> RoutingTable {
      let mut buckets = Vec::<Bucket>::with_capacity(KEY_SIZE);
      for _ in 0..KEY_SIZE {
         buckets.push(Bucket::new());
      }

      RoutingTable { parent_key : parent_key.clone(), buckets : buckets }
   }

   /// Inserts a node in the routing table. Employs least-recently-seen eviction
   pub fn insert_node(&mut self, info : NodeInfo) {
      if let Some(index) = self.bucket_for_node(&info.key) {
         let ref mut bucket = self.buckets[index];
         bucket.entries.retain(|ref stored_info| info.key != stored_info.key);
         bucket.entries.push_back(info);
      }
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
    use std::net;
    use std::str::FromStr;

    #[test]
    fn new_routing_table_is_empty() {
       let parent_key = Hash160::random();
       let table = RoutingTable::new(parent_key);
       let test_hash = Hash160::random();
       
       assert!(table.specific_node(&test_hash).is_none());
    }

    #[test]
    fn inserting_and_retrieving_specific_node() {
       let node_info = NodeInfo { key  : Hash160::random(),
                                  ip   : net::IpAddr::from_str("0.0.0.0").unwrap(),
                                  port : 50000 };

       let mut table = RoutingTable::new(Hash160::random());
       table.insert_node(node_info.clone());
       assert_eq!(table.specific_node(&node_info.key), Some(node_info));
    }
}
