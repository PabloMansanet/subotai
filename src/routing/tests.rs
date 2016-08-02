use super::*;
use std::net;
use std::str::FromStr;
use hash::SubotaiHash;
use hash::HASH_SIZE;
use rand::{thread_rng, Rng};

fn node_info_no_net(id : SubotaiHash) -> NodeInfo {
   NodeInfo {
      id : id,
      address : net::SocketAddr::from_str("0.0.0.0:0").unwrap(),
   }
}

#[test]
fn inserting_and_retrieving_specific_node() {
   let node_info = node_info_no_net(SubotaiHash::random());
   let table = Table::new(SubotaiHash::random());
   table.update_node(node_info.clone());
   assert_eq!(table.specific_node(&node_info.id), Some(node_info));
}

#[test]
fn measuring_table_length() {
   let table = Table::new(SubotaiHash::random());
   let mut conflicts = 0usize;
   for _ in 0..50 {
      match table.update_node(node_info_no_net(SubotaiHash::random())) {
         UpdateResult::CausedConflict(_) => conflicts += 1,
         _ => (),
      };
   }

   assert_eq!(50, table.len() + conflicts);
}

#[test]
fn inserting_in_a_full_bucket_causes_eviction_conflict() {
   let mut parent_id = SubotaiHash::blank();
   parent_id.raw[1] = 1; // This will guarantee all nodes will fall on the same bucket.

   let table = Table::new(parent_id);

   table.fill_bucket(8, super::K_FACTOR as u8);

   // When we add another node to the same bucket, we cause a conflict.
   let mut id = SubotaiHash::blank();
   id.raw[0] = 0xFF;
   let info = node_info_no_net(id);
   match table.update_node(info) {
      UpdateResult::CausedConflict(_) => (),
      _ => panic!(),
   }
}

#[test]
fn lookup_for_a_stored_node() { 
   let table = Table::new(SubotaiHash::random());
   let node = node_info_no_net(SubotaiHash::random());
   table.update_node(node.clone());

   assert_eq!(table.lookup(&node.id, 20, None), LookupResult::Found(node));
}

#[test]
fn lookup_for_self() {
   let parent_id = SubotaiHash::random();
   let table = Table::new(parent_id.clone());
   assert_eq!(table.lookup(&parent_id, 20, None), LookupResult::Myself);
}

#[test]
fn ascending_lookup_on_a_sparse_table() {
   let parent_id = SubotaiHash::random();
   let table = Table::new(parent_id.clone());
   for i in (10..50).filter(|x| x%2 == 0) {
     table.fill_bucket(i, 2);
   }
   let mut id = parent_id;
   id.flip_bit(8); // Bucket 8
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&id, 5, None) {
      assert_eq!(nodes.len(), 5);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.id ^ &id) <= (&next_node.id ^ &id)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

#[test]
fn descending_lookup_on_a_sparse_table() {
   let parent_id = SubotaiHash::random();
   let table = Table::new(parent_id.clone());
   for i in (10..50).filter(|x| x%2 == 0) {
     table.fill_bucket(i, 2);
   }
   let mut id = parent_id;
   id.flip_bit(51); // Bucket 51
   id.raw[0] = 0xFF;
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&id, 5, None) {
      assert_eq!(nodes.len(), 5);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.id ^ &id) <= (&next_node.id ^ &id)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

#[test]
fn lookup_on_a_sparse_table() {
   let parent_id = SubotaiHash::random();
   let table = Table::new(parent_id.clone());
   for i in (10..50).filter(|x| x%2 == 0) {
     table.fill_bucket(i, 2);
   }
   let mut id = parent_id;
   id.flip_bit(25); // Bucket 25
   id.raw[0] = 0xFF;
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&id, 5, None) {
      assert_eq!(nodes.len(), 5);

      // Ensure they are ordered by ascending distance
      for (current_node, next_node) in nodes.iter().zip(nodes.iter().skip(1)) {
         assert!((&current_node.id ^ &id) <= (&next_node.id ^ &id)); 
      }
   }
   else {
      panic!("We shouldn't have found the node!");
   }
}

#[test]
fn lookup_with_blacklist() {
   let table = Table::new(SubotaiHash::random());
   let blacklist = vec![node_info_no_net(SubotaiHash::random()); 5];
   let normal_node = node_info_no_net(SubotaiHash::random());

   for node in &blacklist {
      table.update_node(node.clone());
   }
  
   let blacklist = blacklist.iter().map(|info: &NodeInfo| info.id.clone()).collect::<Vec<SubotaiHash>>();

   table.update_node(normal_node.clone());
   
   if let LookupResult::ClosestNodes(mut nodes) = table.lookup(&SubotaiHash::random(), 5, Some(&blacklist)) {
      assert_eq!(nodes.len(), 1);
      assert_eq!(nodes.pop().unwrap().id, normal_node.id);
   } else {
      panic!("We shouldn't have found the node!");
   }
}

#[test]
fn efficient_bounce_lookup_on_a_randomized_table() {
   let parent_id = SubotaiHash::random();
   let table = Table::new(parent_id.clone());
   for _ in 0..300 {
      // We create ids that will distribute more or less uniformly over the buckets.
      let mut id = parent_id.clone();
      id.mutate_random_bits(3);
      table.update_node(node_info_no_net(id));
   }

   // We construct an origin node from which to calculate distances for the lookup.
   let mut id = parent_id.clone();
   id.mutate_random_bits(20);
   if let LookupResult::ClosestNodes(nodes) = table.lookup(&id, 20, None) {
      assert_eq!(nodes.len(), 20);

      // Ensure they are ordered by ascending distance by comparing to a brute force
      // sorted list of nodes
      let mut ordered_nodes = 
         table
         .all_nodes()
         .collect::<Vec<NodeInfo>>();
      ordered_nodes.sort_by_key(|ref info| &info.id ^ &id);

      for (a, b) in nodes.iter().zip(ordered_nodes.iter()) {
         assert_eq!(a, b);
      }

      // Different way to locate nodes
      let nodes_iterator = table.closest_nodes_to(&id);
      for (a, b) in nodes.into_iter().zip(nodes_iterator) {
         assert_eq!(a,b);
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
         let mut id = self.parent_id.clone();
         id.flip_bit(bucket_index);

         id.raw[0] = i as u8;
         let info = node_info_no_net(id);
         self.update_node(info);
      }
   }
}

impl SubotaiHash {
   pub fn mutate_random_bits(&mut self, number_of_bits : u8) {
      for _ in 0..number_of_bits {
         
         let index = thread_rng().gen::<usize>() % HASH_SIZE;
         self.flip_bit(index);
      }
   }
}

