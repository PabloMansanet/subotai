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

   fn bucket_for_node(&self, key : &Sha1Hash) -> usize {
      let distance = Sha1Hash::xor_distance(&self.parent_key, &key);
      // Finds the last byte that contains a 1.
      let last_byte = distance.raw.iter().enumerate().rev().find(|&pair| pair.0 != 0);
      if let Some((byte, index)) = last_byte {
         for bit in 7..0 {
            if (byte & (1 << bit)) != 0 {
               return (8 * index + bit) as usize
            }
         }
      }
      0
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
