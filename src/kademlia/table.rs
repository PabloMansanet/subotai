use hash::KEY_SIZE;
use hash::Hash160;
use std::net;
use std::collections::VecDeque;
use std::mem;

const K : usize = 20;
const BUCKET_DEPTH : usize = K;

/// Kademlia routing table, with 160 buckets of `BUCKET_DEPTH` (k) node
/// identifiers each.
///
/// The structure employs least-recently seen eviction. Conflicts generated
/// by evicting a node by inserting a newer one remain tracked, so they can
/// can be resolved later.
pub struct RoutingTable {
   parent_key : Hash160,
   buckets    : Vec<Bucket>,
   conflicts  : Vec<EvictionConflict>,
}

/// Bucket size is estimated to be small enough not to warrant
/// the downsides of using a linked list.
#[derive(Debug, Clone)]
struct Bucket {
   entries  : VecDeque<NodeInfo>,
}

/// Represents a conflict derived from attempting to insert a node in a full
/// bucket. 
#[derive(Debug,Clone)]
struct EvictionConflict {
   evicted  : NodeInfo,
   inserted : NodeInfo
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeInfo {
   pub key  : Hash160,
   pub ip   : net::IpAddr,
   pub port : u16,
}

#[derive(Debug, PartialEq)]
pub enum LookupResult {
   Found(NodeInfo), 
   ClosestNodes(Vec<NodeInfo>),
   Myself,
}

impl RoutingTable {
   pub fn new(parent_key : Hash160) -> RoutingTable {
      let mut buckets = Vec::<Bucket>::with_capacity(KEY_SIZE);
      for _ in 0..KEY_SIZE {
         buckets.push(Bucket::new());
      }

      RoutingTable { 
         parent_key : parent_key,
         buckets : buckets,
         conflicts  : Vec::new() 
      }
   }

   /// Inserts a node in the routing table. Employs least-recently-seen eviction
   /// by kicking out the oldest node in case the bucket is full, and registering
   /// an eviction conflict that can be revised later.
   pub fn insert_node(&mut self, info : NodeInfo) {
      if let Some(index) = self.bucket_for_node(&info.key) {
         let bucket = &mut self.buckets[index];
         bucket.entries.retain(|ref stored_info| info.key != stored_info.key);
         if bucket.entries.len() == BUCKET_DEPTH {
            let evicted = bucket.entries.pop_front().unwrap();
            let conflict = EvictionConflict { evicted  : evicted,
                                              inserted : info.clone() };
            self.conflicts.push(conflict);
         }

         bucket.entries.push_back(info);
      }
   }

   /// Performs a node lookup on the routing table. The lookup result may
   /// contain the specific node, a list of up to the N closest nodes, or
   /// report that the parent node itself was requested.
   pub fn lookup(&self, key : &Hash160, n : usize) -> LookupResult {
      if *key == self.parent_key {
         return LookupResult::Myself;
      } 

      match self.specific_node(key) {
         Some(info) => LookupResult::Found(info),
         None => LookupResult::ClosestNodes(self.closest_n_nodes_to(key, n)),
      }
   }

   /// Returns a table entry for the specific node with a given hash.
   fn specific_node(&self, key : &Hash160) -> Option<NodeInfo> {
      if let Some(index) = self.bucket_for_node(key){
         let bucket = &self.buckets[index];
         return bucket.entries.iter().find(|ref info| *key == info.key).cloned();
      }
      None
   }

   fn closest_n_nodes_to(&self, key : &Hash160, n : usize) -> Vec<NodeInfo>{
      let mut closest = Vec::with_capacity(n);
      // Unwrap because we shouldn't be calling this private method
      // with our own key as argument.
      let ideal_bucket_index = self.bucket_for_node(key).unwrap();

      let descent_from_ideal = (1..(ideal_bucket_index+1)).rev();
      let ascent_from_above_ideal = (ideal_bucket_index+1)..KEY_SIZE;
      let lookup_order = descent_from_ideal.chain(ascent_from_above_ideal);

      for bucket_index in lookup_order {
         self.pour_in_order(&mut closest, self.buckets[bucket_index].clone());
         if closest.capacity() == closest.len() {
            break;
         }
      }
      closest
   }

   fn pour_in_order (&self, destination : &mut Vec<NodeInfo>, source : Bucket){
      let mut nodes_from_bucket = source.entries.into_iter().collect::<Vec<NodeInfo>>();
      nodes_from_bucket.sort_by_key(|ref info| &info.key ^ &self.parent_key);

      let space_left = destination.capacity() - destination.len();
      nodes_from_bucket.truncate(space_left);
      destination.append(&mut nodes_from_bucket);
   }

   /// Returns the appropriate position for a node, by computing
   /// the last bit of their distance set to 1. None if we are
   /// attempting to add the parent key.
   fn bucket_for_node(&self, key : &Hash160) -> Option<usize> {
       (&self.parent_key ^ key).height()
   }

   fn revert_conflict(&mut self, conflict : EvictionConflict) {
      if let Some(index) = self.bucket_for_node(&conflict.inserted.key) {
         let bucket  = &mut self.buckets[index];
         let evictor = &mut bucket.entries.iter_mut().find(|ref info| conflict.inserted.key == info.key).unwrap();
         mem::replace::<NodeInfo>(evictor, conflict.evicted);
      }
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
       let node_info = NodeInfo { 
          key  : Hash160::random(),
          ip   : net::IpAddr::from_str("0.0.0.0").unwrap(),
          port : 50000 
       };

       let mut table = RoutingTable::new(Hash160::random());
       table.insert_node(node_info.clone());
       assert_eq!(table.specific_node(&node_info.key), Some(node_info));
    }

    impl RoutingTable {
       pub fn fill_bucket(&mut self, bucket_index : usize, fill_quantity : u8) {
          // Otherwise this helper function becomes quite complex.
          assert!(bucket_index > 7);
          let any_ip = net::IpAddr::from_str("0.0.0.0").unwrap();
          for i in 0..fill_quantity {
             let mut key = self.parent_key.clone();
             key.flip_bit(bucket_index);

             key.raw[0] = i as u8;
             let info = NodeInfo { 
                key  : key,
                ip   : any_ip.clone(),
                port : 0
             };
             self.insert_node(info);
          }
       }
    }

    #[test]
    fn inserting_in_a_full_bucket_causes_eviction_conflict() {
       let mut parent_key = Hash160::blank();
       parent_key.raw[1] = 1; // This will guarantee all nodes will fall on the same bucket.
       let any_ip = net::IpAddr::from_str("0.0.0.0").unwrap();

       let mut table = RoutingTable::new(parent_key);

       table.fill_bucket(8, super::BUCKET_DEPTH as u8);
       assert!(table.conflicts.is_empty());

       // When we add another node to the same bucket, we cause a conflict.
       let mut key = Hash160::blank();
       key.raw[0] = 0xFF;
       let info = NodeInfo { 
          key  : key,
          ip   : any_ip.clone(),
          port : 0
       };
       table.insert_node(info);
       assert_eq!(table.conflicts.len(), 1);

       let conflict = table.conflicts.first().unwrap();
       // We evicted the oldest node, which has a blank key.
       assert_eq!(conflict.evicted.key, Hash160::blank());
       // We inserted the newest node, but it remains referenced in the conflict.
       assert!(table.specific_node(&conflict.inserted.key).is_some());
    }

    #[test]
    fn reverting_an_eviction_conflict_reinserts_the_evicted_node_in_place_of_evictor() {
       let mut parent_key = Hash160::blank();
       parent_key.raw[1] = 1; // This will guarantee all nodes will fall on the same bucket.
       let any_ip = net::IpAddr::from_str("0.0.0.0").unwrap();

       let mut table = RoutingTable::new(parent_key);

       table.fill_bucket(8, super::BUCKET_DEPTH as u8);
       assert!(table.conflicts.is_empty());

       // When we add another node to the same bucket, we cause a conflict.
       let mut key = Hash160::blank();
       key.raw[0] = 0xFF;
       let info = NodeInfo { 
          key  : key,
          ip   : any_ip.clone(),
          port : 0
       };
       table.insert_node(info);
       assert_eq!(table.conflicts.len(), 1);
       let conflict = table.conflicts.pop().unwrap();

       table.revert_conflict(conflict.clone());
       // The evictor has been removed.
       assert!(table.specific_node(&conflict.inserted.key).is_none());
       // And the evicted has been reinserted.
       assert!(table.specific_node(&conflict.evicted.key).is_some());
    }

    #[test]
    fn lookup_for_a_stored_node() { 
       let mut table = RoutingTable::new(Hash160::random());
       let node = NodeInfo {
          key  : Hash160::random(),
          ip   : net::IpAddr::from_str("0.0.0.0").unwrap(),
          port : 0,
       };
       table.insert_node(node.clone());

       assert_eq!(table.lookup(&node.key, 20), LookupResult::Found(node));
    }

    #[test]
    fn lookup_for_a_not_stored_node() {
       let mut table = RoutingTable::new(Hash160::random());
       table.fill_bucket(11, 5);
       table.fill_bucket(12, 5);
       table.fill_bucket(13, 5);

       // We construct a node key that would fall in bucket 12, but isn't stored.
       let mut node_key = table.parent_key.clone();
       node_key.flip_bit(12);
       node_key.raw[0] = 0xFF;

       if let LookupResult::ClosestNodes(nodes) = table.lookup(&node_key,20) {
          assert_eq!(nodes.len(), 15);
       }
       else {
          panic!("We shouldn't have found the node!");
       }
    }
}
