use super::*;
use hash::Hash160;
use hash::KEY_SIZE;
use rand::{thread_rng, Rng};
use std::collections::VecDeque;

fn node_info_no_net(node_id : Hash160) -> NodeInfo {
   NodeInfo {
      node_id : node_id,
      ip   : None,
      port : None
   }
}

#[test]
fn sanity_check() {
   let vecdeque = VecDeque::<u32>::with_capacity(9);
   assert_eq!(vecdeque.capacity(),9);
}

#[test]
fn inserting_and_retrieving_specific_node() {
   let node_info = node_info_no_net(Hash160::random());
   let mut table = RoutingTable::new(Hash160::random());
   table.insert_node(node_info.clone());
   assert_eq!(table.specific_node(&node_info.node_id), Some(node_info));
}

#[test]
fn inserting_in_a_full_bucket_causes_eviction_conflict() {
   let mut parent_node_id = Hash160::blank();
   parent_node_id.raw[1] = 1; // This will guarantee all nodes will fall on the same bucket.

   let mut table = RoutingTable::new(parent_node_id);

   table.fill_bucket(8, super::BUCKET_DEPTH as u8);
   assert!(table.conflicts.is_empty());

   // When we add another node to the same bucket, we cause a conflict.
   let mut node_id = Hash160::blank();
   node_id.raw[0] = 0xFF;
   let info = node_info_no_net(node_id);
   table.insert_node(info);
   assert_eq!(table.conflicts.len(), 1);

   let conflict = table.conflicts.first().unwrap();
   // We evicted the oldest node, which has a blank node_id.
   assert!(table.specific_node(&conflict.inserted.node_id).is_some());
}

#[test]
fn reverting_an_eviction_conflict_reinserts_the_evicted_node_in_place_of_evictor() {
   let mut parent_node_id = Hash160::blank();
   parent_node_id.raw[1] = 1; // This will guarantee all nodes will fall on the same bucket.

   let mut table = RoutingTable::new(parent_node_id);

   table.fill_bucket(8, super::BUCKET_DEPTH as u8);
   assert!(table.conflicts.is_empty());

   // When we add another node to the same bucket, we cause a conflict.
   let mut node_id = Hash160::blank();
   node_id.raw[0] = 0xFF;
   let info = node_info_no_net(node_id);
   table.insert_node(info);
   assert_eq!(table.conflicts.len(), 1);
   let conflict = table.conflicts.pop().unwrap();

   table.revert_conflict(conflict.clone());
   // The evictor has been removed.
   assert!(table.specific_node(&conflict.inserted.node_id).is_none());
   // And the evicted has been reinserted.
   assert!(table.specific_node(&conflict.evicted.node_id).is_some());
}

#[test]
fn lookup_for_a_stored_node() { 
   let mut table = RoutingTable::new(Hash160::random());
   let node = node_info_no_net(Hash160::random());
   table.insert_node(node.clone());

   assert_eq!(table.lookup(&node.node_id, 20), LookupResult::Found(node));
}

#[test]
fn lookup_for_self() {
   let parent_node_id = Hash160::random();
   let table = RoutingTable::new(parent_node_id.clone());
   assert_eq!(table.lookup(&parent_node_id, 20), LookupResult::Found(node_info_no_net(parent_node_id)));
}

#[test]
fn ascending_lookup_on_a_sparse_table() {
   let parent_node_id = Hash160::random();
   let mut table = RoutingTable::new(parent_node_id.clone());
   for i in (10..50).filter(|x| x%2 == 0) {
     table.fill_bucket(i, 2);
   }
   let mut node_id = parent_node_id;
   node_id.flip_bit(8); // Bucket 8
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&node_id,5) {
      assert_eq!(nodes.len(), 5);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.node_id ^ &node_id) <= (&next_node.node_id ^ &node_id)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

#[test]
fn descending_lookup_on_a_sparse_table() {
   let parent_node_id = Hash160::random();
   let mut table = RoutingTable::new(parent_node_id.clone());
   for i in (10..50).filter(|x| x%2 == 0) {
     table.fill_bucket(i, 2);
   }
   let mut node_id = parent_node_id;
   node_id.flip_bit(51); // Bucket 51
   node_id.raw[0] = 0xFF;
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&node_id,5) {
      assert_eq!(nodes.len(), 5);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.node_id ^ &node_id) <= (&next_node.node_id ^ &node_id)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

#[test]
fn lookup_on_a_sparse_table() {
   let parent_node_id = Hash160::random();
   let mut table = RoutingTable::new(parent_node_id.clone());
   for i in (10..50).filter(|x| x%2 == 0) {
     table.fill_bucket(i, 2);
   }
   let mut node_id = parent_node_id;
   node_id.flip_bit(25); // Bucket 25
   node_id.raw[0] = 0xFF;
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&node_id,5) {
      assert_eq!(nodes.len(), 5);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.node_id ^ &node_id) <= (&next_node.node_id ^ &node_id)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

#[test]
fn lookup_on_a_randomized_table() {
   let parent_node_id = Hash160::random();
   let mut table = RoutingTable::new(parent_node_id.clone());
   for _ in 0..300 {
      // We create node_ids that will distribute more or less uniformly over the buckets.
      let mut node_id = parent_node_id.clone();
      node_id.mutate_random_bits(3);
      table.insert_node(node_info_no_net(node_id));
   }
   let mut node_id = parent_node_id.clone();
   node_id.mutate_random_bits(20);
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&node_id,30) {
      assert_eq!(nodes.len(), 30);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.node_id ^ &node_id) <= (&next_node.node_id ^ &node_id)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

impl RoutingTable {
   pub fn fill_bucket(&mut self, bucket_index : usize, fill_quantity : u8) {
      // Otherwise this helper function becomes quite complex.
      assert!(bucket_index > 7);
      let parent_node_id = self.buckets[0].entries[0].node_id.clone();
      for i in 0..fill_quantity {
         let mut node_id = parent_node_id.clone();
         node_id.flip_bit(bucket_index);

         node_id.raw[0] = i as u8;
         let info = node_info_no_net(node_id);
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

