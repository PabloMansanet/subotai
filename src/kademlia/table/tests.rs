use super::*;
use hash::Hash160;
use hash::KEY_SIZE;
use std::net;
use std::str::FromStr;
use rand::{thread_rng, Rng};

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

impl Hash160 {
   pub fn mutate_random_bits(&mut self, number_of_bits : u8) {
      for _ in 0..number_of_bits {
         
         let index = thread_rng().gen::<usize>() % KEY_SIZE;
         self.flip_bit(index);
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
fn lookup_for_self() {
   let parent_key = Hash160::random();
   let table = RoutingTable::new(parent_key.clone());
   assert_eq!(table.lookup(&parent_key, 20), LookupResult::Myself);
}

#[test]
fn ascending_lookup_on_a_sparse_table() {
   let mut table = RoutingTable::new(Hash160::random());
   for i in (10..50).filter(|x| x%2 == 0) {
     table.fill_bucket(i, 2);
   }
   let mut node_key = table.parent_key.clone();
   node_key.flip_bit(8); // Bucket 8
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&node_key,5) {
      assert_eq!(nodes.len(), 5);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.key ^ &node_key) <= (&next_node.key ^ &node_key)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

#[test]
fn descending_lookup_on_a_sparse_table() {
   let mut table = RoutingTable::new(Hash160::random());
   for i in (10..50).filter(|x| x%2 == 0) {
     table.fill_bucket(i, 2);
   }
   let mut node_key = table.parent_key.clone();
   node_key.flip_bit(51); // Bucket 51
   node_key.raw[0] = 0xFF;
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&node_key,5) {
      assert_eq!(nodes.len(), 5);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.key ^ &node_key) <= (&next_node.key ^ &node_key)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

#[test]
fn lookup_on_a_sparse_table() {
   let mut table = RoutingTable::new(Hash160::random());
   for i in (10..50).filter(|x| x%2 == 0) {
     table.fill_bucket(i, 2);
   }
   let mut node_key = table.parent_key.clone();
   node_key.flip_bit(25); // Bucket 25
   node_key.raw[0] = 0xFF;
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&node_key,5) {
      assert_eq!(nodes.len(), 5);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.key ^ &node_key) <= (&next_node.key ^ &node_key)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

#[test]
fn lookup_on_a_randomized_table() {
   let mut table = RoutingTable::new(Hash160::random());
   let any_ip = net::IpAddr::from_str("0.0.0.0").unwrap();
   for _ in 0..3000 {
      // We create keys that will distribute more or less uniformly over the buckets.
      let mut node_key = table.parent_key.clone();
      node_key.mutate_random_bits(3);
      table.insert_node(NodeInfo{
         key  : node_key,
         ip   : any_ip.clone(),
         port : 0
      });
   }
   let mut node_key = table.parent_key.clone();
   node_key.mutate_random_bits(20);
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&node_key,30) {
      assert_eq!(nodes.len(), 30);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.key ^ &node_key) <= (&next_node.key ^ &node_key)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}
