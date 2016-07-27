use hash;
use hash::HASH_SIZE;
use hash::Hash;
use std::{net, mem, sync, iter};
use std::collections::VecDeque;

#[cfg(test)]
mod tests;

/// System-wide concurrency factor. It's used, for example, to decide the
/// number of remote nodes to interrogate concurrently when performing a 
/// network-wide lookup.
pub const ALPHA    : usize = 3;

/// Impatience factor, valid in the range [0..ALPHA). When performing "waves",
/// the impatience factor denotes how many nodes we can give up waiting for, before
/// starting the next wave. 
///
/// If we send a request to ALPHA nodes during a lookup wave, we will start
/// the next wave after we receive 'ALPHA - IMPATIENCE' responses.
pub const IMPATIENCE    : usize = 1;

/// Data structure factor. It's used, among other places, to dictate the 
/// size of a K-bucket.
pub const K        : usize = 20;
const BUCKET_DEPTH : usize = K;

/// Routing table with 160 buckets of `BUCKET_DEPTH` (k) node
/// identifiers each, constructed around a parent node ID.
///
/// The structure employs least-recently seen eviction. Conflicts generated
/// by evicting a node by inserting a newer one remain tracked, so they can
/// be resolved later.
pub struct Table {
   buckets   : Vec<Bucket>,
   conflicts : sync::Mutex<Vec<EvictionConflict>>,
   parent_id : Hash,
}

/// ID - Address pair that identifies a unique Subotai node in the network.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct NodeInfo {
   pub id      : Hash,
   pub address : net::SocketAddr,
}

/// Result of a table lookup. 
/// * `Myself`: The requested ID is precisely the parent node.
///
/// * `Found`: The requested ID was found on the table.
///
/// * `ClosestNodes`: The requested ID was not found, but here are the next
///   closest nodes to consult.
/// 
/// * `Nothing`: The table is empty or the blacklist provided doesn't allow 
///   returning any close nodes.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum LookupResult {
   Myself,
   Found(NodeInfo), 
   ClosestNodes(Vec<NodeInfo>),
   Nothing,
}

impl Table {
   /// Constructs a routing table based on a parent node id. Other nodes
   /// will be stored in this table based on their distance to the node id provided.
   pub fn new(parent_id: Hash) -> Table {
      Table { 
         buckets   : (0..HASH_SIZE).map(|_| Bucket::new()).collect(),
         conflicts : sync::Mutex::new(Vec::new()),
         parent_id : parent_id,
      }
   }

   /// Returns the number of nodes currently on the table.
   pub fn len(&self) -> usize {
      self.buckets.iter().map(|bucket| bucket.entries.read().unwrap().len()).sum()
   }

   pub fn is_empty(&self) -> bool {
      self.len() == 0
   }

   /// Inserts a node in the routing table. Employs least-recently-seen eviction
   /// by kicking out the oldest node in case the bucket is full, and registering
   /// an eviction conflict that can be revised later.
   pub fn insert_node(&self, info: NodeInfo) {
      if let Some(index) = self.bucket_for_node(&info.id) {
         let bucket = &self.buckets[index];
         let mut entries = bucket.entries.write().unwrap();

         entries.retain(|ref stored_info| info.id != stored_info.id);
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
   pub fn lookup(&self, id: &Hash, n: usize, blacklist: Option<&Vec<Hash>>) -> LookupResult {
      if id == &self.parent_id {
         return LookupResult::Myself;
      }

      match self.specific_node(id) {
         Some(info) => LookupResult::Found(info),
         None =>  {
            let closest: Vec<NodeInfo> = self.closest_nodes_to(id)
               .filter(|ref info| Self::is_allowed(&info.id, blacklist))
               .take(n)
               .collect();

            if closest.is_empty() {
               LookupResult::Nothing
            } else {
               LookupResult::ClosestNodes(closest)
            }
         }
      }
   }

   fn is_allowed(id: &Hash, blacklist: Option<&Vec<Hash>>) -> bool {
      if let Some(blacklist) = blacklist {
         !blacklist.contains(id)
      } else {
         true
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
         table          : self,
         current_bucket : Vec::with_capacity(BUCKET_DEPTH),
         bucket_index   : 0,
      }
   }

   /// Returns an iterator over all stored nodes, ordered by ascending
   /// distance to a given reference ID. This iterator is designed for concurrent
   /// access to the data structure, and as such it isn't guaranteed that it
   /// will return a "snapshot" of all nodes for a specific moment in time. 
   /// Buckets already visited may be modified elsewhere through iteraton, 
   /// and unvisited buckets may accrue new nodes.
   pub fn closest_nodes_to<'a,'b>(&'a self, id: &'b Hash) -> ClosestNodesTo<'a,'b> {
      let distance = &self.parent_id ^ id;
      let descent  = distance.clone().into_ones().rev();
      let ascent   = distance.into_zeroes();
      let lookup_order = descent.chain(ascent);

      ClosestNodesTo {
         table          : self,
         reference      : id,
         lookup_order   : lookup_order,
         current_bucket : Vec::with_capacity(BUCKET_DEPTH),
      }
   }

   /// Returns a table entry for the specific node with a given hash.
   pub fn specific_node(&self, id: &Hash) -> Option<NodeInfo> {
      if let Some(index) = self.bucket_for_node(id) {
         let entries = &self.buckets[index].entries.read().unwrap();
         return entries.iter().find(|ref info| *id == info.id).cloned();
      }
      None
   }

   /// Returns the appropriate position for a node, by computing
   /// the index where their prefix starts differing. If we are requesting
   /// the bucket for this table's own parent node, it can't be stored.
   fn bucket_for_node(&self, id: &Hash) -> Option<usize> {
       (&self.parent_id ^ id).height()
   }

   fn revert_conflict(&self, conflict: EvictionConflict) {
      if let Some(index) = self.bucket_for_node(&conflict.inserted.id) {
         let mut entries = self.buckets[index].entries.write().unwrap();
         if let Some(ref mut evictor) = entries.iter_mut().find(|ref info| conflict.inserted.id == info.id) {
            mem::replace::<NodeInfo>(evictor, conflict.evicted);
         } else {
            self.insert_node(conflict.evicted);
         }
      }
   }
}

/// Produces copies of all known nodes, ordered in ascending
/// distance from self. The table may be modified through iteration.
pub struct AllNodes<'a> {
   table          : &'a Table,
   current_bucket : Vec<NodeInfo>,
   bucket_index   : usize,
}

/// Produces copies of all known nodes, ordered in ascending
/// distance from a reference ID.
pub struct ClosestNodesTo<'a, 'b> {
   table          : &'a Table,
   reference      : &'b hash::Hash,     
   lookup_order   : iter::Chain<iter::Rev<hash::IntoOnes>, hash::IntoZeroes>,
   current_bucket : Vec<NodeInfo>,
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
   entries: sync::RwLock<VecDeque<NodeInfo>>,
}

impl<'a, 'b> Iterator for ClosestNodesTo<'a, 'b> {
   type Item = NodeInfo;

   fn next(&mut self) -> Option<NodeInfo> {
      if !self.current_bucket.is_empty() {
         return self.current_bucket.pop();
      }

      while let Some(index) = self.lookup_order.next() {
         let mut new_bucket = { // Lock scope
            let bucket = self.table.buckets[index].entries.read().unwrap();
            if bucket.is_empty() {
               continue;
            }
            bucket.clone()
         }.into_iter().collect::<Vec<NodeInfo>>();

         new_bucket.sort_by(|ref info_a, ref info_b| (&info_b.id ^ self.reference).cmp(&(&info_a.id ^ self.reference)));
         self.current_bucket.append(&mut new_bucket);
         return self.current_bucket.pop();
      }
      None
   }
}

impl<'a> Iterator for AllNodes<'a> {
   type Item = NodeInfo;

   fn next(&mut self) -> Option<NodeInfo> {
      while self.bucket_index < HASH_SIZE && self.current_bucket.is_empty() {
         let mut new_bucket = { // Lock scope
            self.table.buckets[self.bucket_index].entries.read().unwrap().clone()
         }.into_iter().collect::<Vec<NodeInfo>>();

         new_bucket.sort_by_key(|ref info| &info.id ^ &self.table.parent_id);
         self.current_bucket.append(&mut new_bucket);
         self.bucket_index += 1;
      }
      self.current_bucket.pop()
   } 
}

impl Bucket {
   fn new() -> Bucket {
      Bucket{
         entries: sync::RwLock::new(VecDeque::with_capacity(BUCKET_DEPTH))
      }
   }
}
