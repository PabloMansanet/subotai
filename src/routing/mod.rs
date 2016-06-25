use hash::HASH_SIZE;
use hash::Hash;
use std::net;
use std::collections::VecDeque;
use std::mem;
use std::sync::Mutex;

#[cfg(test)]
mod tests;

const K : usize = 20;
const BUCKET_DEPTH : usize = K;

/// Kademlia routing table, with 160 buckets of `BUCKET_DEPTH` (k) node
/// identifiers each, constructed around a parent node ID.
///
/// The structure employs least-recently seen eviction. Conflicts generated
/// by evicting a node by inserting a newer one remain tracked, so they can
/// be resolved later.
pub struct Table {
   buckets        : Vec<Bucket>,
   conflicts      : Mutex<Vec<EvictionConflict>>,
   parent_node_id : Hash,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeInfo {
   pub node_id : Hash,
   pub address : net::SocketAddr,
}

#[derive(Debug, PartialEq)]
pub enum LookupResult {
   Myself,
   Found(NodeInfo), 
   ClosestNodes(Vec<NodeInfo>),
}

impl Table {

   /// Constructs a routing table based on a parent node id. Other nodes
   /// will be stored in this table based on their distance to the node id provided.
   pub fn new(parent_node_id: Hash) -> Table {
      let mut buckets = Vec::<Bucket>::with_capacity(HASH_SIZE);
      for _ in 0..HASH_SIZE {
         buckets.push(Bucket::new());
      }

      Table { 
         buckets        : buckets,
         conflicts      : Mutex::new(Vec::new()),
         parent_node_id : parent_node_id,
      }
   }

   /// Inserts a node in the routing table. Employs least-recently-seen eviction
   /// by kicking out the oldest node in case the bucket is full, and registering
   /// an eviction conflict that can be revised later.
   pub fn insert_node(&self, info: NodeInfo) {
      if let Some(index) = self.bucket_for_node(&info.node_id) {
         let bucket = &self.buckets[index];
         let mut entries = bucket.entries.lock().unwrap();

         entries.retain(|ref stored_info| info.node_id != stored_info.node_id);
         if entries.len() == BUCKET_DEPTH {
            let conflict = EvictionConflict { 
               evicted  : entries.pop_front().unwrap(),
               inserted : info.clone() 
            };
            let mut conflicts = self.conflicts.lock().unwrap();
            conflicts.push(conflict);
         }
         entries.push_back(info);
      }
   }

   /// Performs a node lookup on the routing table. The lookup result may
   /// contain the specific node, a list of up to the N closest nodes, or
   /// report that the parent node itself was requested.
   ///
   /// This employs an algorithm I have named "bounce lookup", which obtains
   /// the closest nodes to a given origin walking through the minimum 
   /// amount of buckets. It may exist already, but I haven't 
   /// found it any other implementation. It consists of:
   ///
   /// * Calculating the XOR distance between the parent node ID and the 
   ///   lookup node ID.
   ///
   /// * Checking the buckets indexed by the position of every "1" in said
   ///   distance hash, in descending order.
   ///
   /// * "Bounce" back up, checking the buckets indexed by the position of
   ///   every "0" in that distance hash, in ascending order.
   pub fn lookup(&self, node_id: &Hash, n: usize) -> LookupResult {
      if node_id == &self.parent_node_id {
         return LookupResult::Myself;
      }

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
         table          : &self,
         current_bucket : Vec::with_capacity(BUCKET_DEPTH),
         bucket_index   : 0,
      }
   }

   /// Returns a table entry for the specific node with a given hash.
   pub fn specific_node(&self, node_id: &Hash) -> Option<NodeInfo> {
      if let Some(index) = self.bucket_for_node(node_id) {
         let entries = &self.buckets[index].entries.lock().unwrap();
         return entries.iter().find(|ref info| *node_id == info.node_id).cloned();
      }
      None
   }

   /// Bounce lookup algorithm.
   fn closest_n_nodes_to(&self, node_id: &Hash, n: usize) -> Vec<NodeInfo> {
      let mut closest = Vec::with_capacity(n);
      let distance = &self.parent_node_id ^ node_id;
      let descent  = distance.ones().rev();
      let ascent   = distance.zeroes();
      let lookup_order = descent.chain(ascent);
      
      for bucket_index in lookup_order {
         let entries = self.buckets[bucket_index].entries.lock().unwrap();
         if entries.is_empty() {
            continue;
         }
         
         let mut nodes_from_bucket = entries.clone().into_iter().collect::<Vec<NodeInfo>>();
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
   /// the bucket for this table's own parent node, it can't be stored.
   fn bucket_for_node(&self, node_id: &Hash) -> Option<usize> {
       (&self.parent_node_id ^ node_id).height()
   }

   fn revert_conflict(&self, conflict: EvictionConflict) {
      if let Some(index) = self.bucket_for_node(&conflict.inserted.node_id) {
         let mut entries = self.buckets[index].entries.lock().unwrap();
         let evictor = &mut entries.iter_mut().find(|ref info| conflict.inserted.node_id == info.node_id).unwrap();
         mem::replace::<NodeInfo>(evictor, conflict.evicted);
      }
   }
}

/// Produces copies of all known nodes, ordered in ascending
/// distance from self. It's a weak invariant, i.e. the table
/// may be modified through iteration.
pub struct AllNodes<'a> {
   table          : &'a Table,
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
///
/// Each vector of bucket entries is protected under its own mutex, to guarantee 
/// concurrent access to the table.
#[derive(Debug)]
struct Bucket {
   entries: Mutex<VecDeque<NodeInfo>>,
}

impl<'a> Iterator for AllNodes<'a> {
   type Item = NodeInfo;

   fn next(&mut self) -> Option<NodeInfo> {
      while self.bucket_index < HASH_SIZE && self.current_bucket.is_empty() {
         let mut new_bucket = { // Lock scope
            self.table.buckets[self.bucket_index].entries.lock().unwrap().clone()
         }.into_iter().collect::<Vec<NodeInfo>>();

         new_bucket.sort_by_key(|ref info| &info.node_id ^ &self.table.parent_node_id);
         self.current_bucket.append(&mut new_bucket);
         self.bucket_index += 1;
      }
      self.current_bucket.pop()
   } 
}

impl Bucket {
   fn new() -> Bucket {
      Bucket{
         entries: Mutex::new(VecDeque::with_capacity(BUCKET_DEPTH))
      }
   }
}
