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
   buckets        : Vec<Bucket>,
   conflicts      : Vec<EvictionConflict>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeInfo {
   pub node_id : Hash160,
   pub ip      : Option<net::IpAddr>,
   pub port    : Option<u16>,
}

#[derive(Debug, PartialEq)]
pub enum LookupResult {
   Found(NodeInfo), 
   ClosestNodes(Vec<NodeInfo>),
}

impl RoutingTable {
   /// Constructs a routing table based on a parent node id. Other nodes
   /// will be stored in this table based on their distance to the node id provided.
   pub fn new(parent_node_id : Hash160) -> RoutingTable {
      let mut buckets = Vec::<Bucket>::with_capacity(KEY_SIZE);
      for _ in 0..KEY_SIZE {
         buckets.push(Bucket::new());
      }

      let mut table = RoutingTable { 
         buckets : buckets,
         conflicts  : Vec::new() 
      };

      // Store the parent node at entry zero.
      table.buckets[0].entries.push_back( NodeInfo { 
         node_id : parent_node_id, 
         ip  : None, 
         port : None
      });
      table
   }

   /// Inserts a node in the routing table. Employs least-recently-seen eviction
   /// by kicking out the oldest node in case the bucket is full, and registering
   /// an eviction conflict that can be revised later.
   pub fn insert_node(&mut self, info : NodeInfo) {
      let index = self.bucket_for_node(&info.node_id);
      let bucket = &mut self.buckets[index];
      bucket.entries.retain(|ref stored_info| info.node_id != stored_info.node_id);
      if bucket.entries.len() == BUCKET_DEPTH {
         let conflict = EvictionConflict { 
            evicted  : bucket.entries.pop_front().unwrap(),
            inserted : info.clone() 
         };
         self.conflicts.push(conflict);
      }
      bucket.entries.push_back(info);
   }

   /// Performs a node lookup on the routing table. The lookup result may
   /// contain the specific node, a list of up to the N closest nodes, or
   /// report that the parent node itself was requested.
   ///
   /// This employs an algorithm I have named "bounce lookup", which obtains
   /// the closest nodes to a given origin walking through the minimum 
   /// amount of buckets. Please let me know if it exists already, haven't 
   /// found it any other implementation. It consists of:
   /// * Calculating the XOR distance between the parent node ID and the 
   /// lookup node ID.
   /// * Checking the buckets indexed by the position of every "1" in said
   /// distance hash, in descending order.
   /// * "Bounce" back up, checking the buckets indexed by the position of
   /// every "0" in that distance hash, in ascending order.
   pub fn lookup(&self, node_id : &Hash160, n : usize) -> LookupResult {
      match self.specific_node(node_id) {
         Some(info) => LookupResult::Found(info),
         None => LookupResult::ClosestNodes(self.closest_n_nodes_to(node_id, n)),
      }
   }

   /// Returns an iterator over all stored nodes, ordered by ascending
   /// distance to the parent node. This iterator is designed for concurrent
   /// access to the data structure, and as such it isn't guaranteed that it
   /// will return a "snapshot" of all nodes for a specific moment in time. 
   /// Buckets already visited may be modified elsewhere through iteraton, 
   /// and unvisited buckets may accrue new nodes.
   pub fn all_nodes(&self) -> AllNodes {
      AllNodes {
         routing_table  : &self,
         current_bucket : Vec::with_capacity(BUCKET_DEPTH),
         bucket_index   : 0,
      }
   }

   /// Returns a table entry for the specific node with a given hash.
   fn specific_node(&self, node_id : &Hash160) -> Option<NodeInfo> {
      let index = self.bucket_for_node(node_id);
      let bucket = &self.buckets[index];
      bucket.entries.iter().find(|ref info| *node_id == info.node_id).cloned()
   }

   /// Bounce lookup algorithm.
   fn closest_n_nodes_to(&self, node_id : &Hash160, n : usize) -> Vec<NodeInfo> {
      let mut closest = Vec::with_capacity(n);
      let parent_node_id = &self.buckets[0].entries[0].node_id;
      let distance = parent_node_id ^ node_id;
      let descent  = distance.clone().ones().rev();
      let ascent   = distance.zeroes();
      let lookup_order = descent.chain(ascent);
      
      for bucket_index in lookup_order {
         if self.buckets[bucket_index].entries.is_empty() {
            continue;
         }
         
         let mut nodes_from_bucket = self.buckets[bucket_index].entries.clone().into_iter().collect::<Vec<NodeInfo>>();
         nodes_from_bucket.sort_by_key(|ref info| &info.node_id ^ node_id);
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
   /// the index where their prefix starts differing. If we are requesting
   /// the bucket for this table's own parent node, we point at bucket 0.
   fn bucket_for_node(&self, node_id : &Hash160) -> usize {
      let parent_node_id = &self.buckets[0].entries[0].node_id;
       match (parent_node_id ^ node_id).height() {
          Some(bucket) => bucket,
          None => 0
       }
   }

   fn revert_conflict(&mut self, conflict : EvictionConflict) {
      let index = self.bucket_for_node(&conflict.inserted.node_id);
      let bucket  = &mut self.buckets[index];
      let evictor = &mut bucket.entries.iter_mut().find(|ref info| conflict.inserted.node_id == info.node_id).unwrap();
      mem::replace::<NodeInfo>(evictor, conflict.evicted);
   }
}

/// Produces copies of all known nodes, ordered in ascending
/// distance from self. It's a weak invariant, i.e. the table
/// may be modified through iteration.
pub struct AllNodes<'a> {
   routing_table  : &'a RoutingTable,
   current_bucket : Vec<NodeInfo>,
   bucket_index   : usize,
}

/// Represents a conflict derived from attempting to insert a node in a full
/// bucket. 
#[derive(Debug,Clone)]
struct EvictionConflict {
   evicted  : NodeInfo,
   inserted : NodeInfo
}

/// Bucket size is estimated to be small enough not to warrant
/// the downsides of using a linked list.
#[derive(Debug, Clone)]
struct Bucket {
   entries  : VecDeque<NodeInfo>,
}

impl<'a> Iterator for AllNodes<'a> {
   type Item = NodeInfo;

   fn next(&mut self) -> Option<NodeInfo> {
      while self.bucket_index < KEY_SIZE && self.current_bucket.is_empty() {
         let mut new_bucket = self.routing_table.buckets[self.bucket_index].entries.clone().into_iter().collect::<Vec<NodeInfo>>();
         new_bucket.sort_by_key(|ref info| &info.node_id ^ &self.routing_table.buckets[0].entries[0].node_id);
         self.current_bucket.append(&mut new_bucket);
         self.bucket_index += 1;
      }
      self.current_bucket.pop()
   } 
}

impl Bucket {
   fn new() -> Bucket {
      Bucket{
         entries: VecDeque::with_capacity(BUCKET_DEPTH)
      }
   }
}
