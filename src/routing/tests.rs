use super::*;
use hash::Hash;
use hash::HASH_SIZE;
use rand::{thread_rng, Rng};

fn node_info_no_net(node_id : Hash) -> NodeInfo {
   NodeInfo {
      node_id : node_id,
      address : None,
   }
}

#[test]
fn inserting_and_retrieving_specific_node() {
   let node_info = node_info_no_net(Hash::random());
   let table = Table::new(Hash::random());
   table.insert_node(node_info.clone());
   assert_eq!(table.specific_node(&node_info.node_id), Some(node_info));
}

#[test]
fn inserting_in_a_full_bucket_causes_eviction_conflict() {
   let mut parent_node_id = Hash::blank();
   parent_node_id.raw[1] = 1; // This will guarantee all nodes will fall on the same bucket.

   let table = Table::new(parent_node_id);

   table.fill_bucket(8, super::BUCKET_DEPTH as u8);
   assert!(table.conflicts.lock().unwrap().is_empty());

   // When we add another node to the same bucket, we cause a conflict.
   let mut node_id = Hash::blank();
   node_id.raw[0] = 0xFF;
   let info = node_info_no_net(node_id);
   table.insert_node(info);
   let conflicts = table.conflicts.lock().unwrap();
   assert_eq!(conflicts.len(), 1); 

   let conflict = conflicts.first().unwrap(); 
   // We evicted the oldest node, which has a blank node_id.
   assert!(table.specific_node(&conflict.inserted.node_id).is_some());
}

#[test]
fn reverting_an_eviction_conflict_reinserts_the_evicted_node_in_place_of_evictor() {
   let mut parent_node_id = Hash::blank();
   parent_node_id.raw[1] = 1; // This will guarantee all nodes will fall on the same bucket.

   let table = Table::new(parent_node_id);

   table.fill_bucket(8, super::BUCKET_DEPTH as u8);
   assert!(table.conflicts.lock().unwrap().is_empty());

   // When we add another node to the same bucket, we cause a conflict.
   let mut node_id = Hash::blank();
   node_id.raw[0] = 0xFF;
   let info = node_info_no_net(node_id);
   table.insert_node(info);
   assert_eq!(table.conflicts.lock().unwrap().len(), 1);
   let conflict = table.conflicts.lock().unwrap().pop().unwrap();

   table.revert_conflict(conflict.clone());
   // The evictor has been removed.
   assert!(table.specific_node(&conflict.inserted.node_id).is_none());
   // And the evicted has been reinserted.
   assert!(table.specific_node(&conflict.evicted.node_id).is_some());
}

#[test]
fn lookup_for_a_stored_node() { 
   let table = Table::new(Hash::random());
   let node = node_info_no_net(Hash::random());
   table.insert_node(node.clone());

   assert_eq!(table.lookup(&node.node_id, 20), LookupResult::Found(node));
}

#[test]
fn lookup_for_self() {
   let parent_node_id = Hash::random();
   let table = Table::new(parent_node_id.clone());
   assert_eq!(table.lookup(&parent_node_id, 20), LookupResult::Myself);
}

#[test]
fn ascending_lookup_on_a_sparse_table() {
   let parent_node_id = Hash::random();
   let table = Table::new(parent_node_id.clone());
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
   let parent_node_id = Hash::random();
   let table = Table::new(parent_node_id.clone());
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
   let parent_node_id = Hash::random();
   let table = Table::new(parent_node_id.clone());
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
fn efficient_bounce_lookup_on_a_randomized_table() {
   let parent_node_id = Hash::random();
   let table = Table::new(parent_node_id.clone());
   for _ in 0..300 {
      // We create node_ids that will distribute more or less uniformly over the buckets.
      let mut node_id = parent_node_id.clone();
      node_id.mutate_random_bits(3);
      table.insert_node(node_info_no_net(node_id));
   }

   // We construct an origin node from which to calculate distances for the lookup.
   let mut node_id = parent_node_id.clone();
   node_id.mutate_random_bits(20);
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&node_id, 20) {
      assert_eq!(nodes.len(), 20);

      // Ensure they are ordered by ascending distance by comparing to a brute force
      // sorted list of nodes
      let mut ordered_nodes = 
         table
         .all_nodes()
         .collect::<Vec<NodeInfo>>();
      ordered_nodes.sort_by_key(|ref info| &info.node_id ^ &node_id);

      for (a, b) in nodes.iter().zip(ordered_nodes.iter()) {
         assert_eq!(a, b);
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

impl Table {
   pub fn fill_bucket(&self, bucket_index : usize, fill_quantity : u8) {
      // Otherwise this helper function becomes quite complex.
      assert!(bucket_index > 7);
      for i in 0..fill_quantity {
         let mut node_id = self.parent_node_id.clone();
         node_id.flip_bit(bucket_index);

         node_id.raw[0] = i as u8;
         let info = node_info_no_net(node_id);
         self.insert_node(info);
      }
   }
}

impl Hash {
   pub fn mutate_random_bits(&mut self, number_of_bits : u8) {
      for _ in 0..number_of_bits {
         
         let index = thread_rng().gen::<usize>() % HASH_SIZE;
         self.flip_bit(index);
      }
   }
}

