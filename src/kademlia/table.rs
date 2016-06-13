use hash::KEY_SIZE;
use hash::Sha1Hash;
use std::net;
use std::collections::VecDeque;

const BUCKET_DEPTH : usize = 20;

pub struct RoutingTable {
   parent_key : Sha1Hash,
   buckets : Vec<Bucket>
}

struct Bucket {
   entries  : VecDeque<NodeInfo>
}

pub struct NodeInfo {
   key  : Sha1Hash,
   ip   : net::IpAddr,
   port : u16,
}

impl RoutingTable {
   pub fn new(parent_key : &Sha1Hash) -> RoutingTable {
      let mut buckets = Vec::<Bucket>::with_capacity(KEY_SIZE);
      for _ in 0..KEY_SIZE {
         buckets.push(Bucket::new());
      }

      RoutingTable { parent_key : parent_key.clone(), buckets : buckets }
   }

   fn specific_node(&self, key : &Sha1Hash) -> Option<NodeInfo> {
      None
   }

   /// Returns the appropriate position for a node, by computing
   /// the last bit of their distance set to 1.
   fn bucket_for_node(&self, key : &Sha1Hash) -> usize {
      let distance = Sha1Hash::xor_distance(&self.parent_key, &key);
      match distance.index_highest_1() {
         Some(index) => index,
         None => 0
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
    use hash::Sha1Hash;

    #[test]
    fn new_routing_table_is_empty() {
       let parent_key = Sha1Hash::from_string("");
       let table = RoutingTable::new(&parent_key);
       let test_hash = Sha1Hash::from_string("");
       
       assert!(table.specific_node(&test_hash).is_none());
    }
}
