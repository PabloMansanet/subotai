use hash::KEY_SIZE;
use hash::Hash160;
use std::net;
use std::collections::VecDeque;
use std::mem;

#[cfg(test)]
mod tests;

const K : usize = 20;
const BUCKET_DEPTH : usize = K;

/// Kademlia routing table, with 160 buckets of `BUCKET_DEPTH` (k) node
/// identifiers each.
///
/// The structure employs least-recently seen eviction. Conflicts generated
/// by evicting a node by inserting a newer one remain tracked, so they can
/// be resolved later.
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
            let conflict = EvictionConflict { 
               evicted  : bucket.entries.pop_front().unwrap(),
               inserted : info.clone() 
            };
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

   fn closest_n_nodes_to(&self, key : &Hash160, n : usize) -> Vec<NodeInfo> {
      let mut closest = Vec::with_capacity(n);
      let distance = &self.parent_key ^ key;
      let descent  = distance.clone().ones().rev();
      let ascent   = distance.clone().zeroes();
      let lookup_order = descent.chain(ascent);

      for bucket_index in lookup_order {
         if self.buckets[bucket_index].entries.len() == 0 {
            continue;
         }
         
         let mut nodes_from_bucket = self.buckets[bucket_index].entries.clone().into_iter().collect::<Vec<NodeInfo>>();
         println!("Trying bucket {} with {} entries", bucket_index, nodes_from_bucket.len());
         nodes_from_bucket.sort_by_key(|ref info| &info.key ^ key);
         let space_left = closest.capacity() - closest.len();
         nodes_from_bucket.truncate(space_left);
         closest.append(&mut nodes_from_bucket);

         if closest.len() == closest.capacity() {
            break;
         }
      }
      closest
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
