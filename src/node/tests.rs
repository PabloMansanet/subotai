use {node, routing, time, hash, storage};
use std::collections::VecDeque;
use std::str::FromStr;
use std::net;
use node::receptions;

pub const POLL_FREQUENCY_MS: u64 = 50;
pub const TRIES: u8 = 5;

#[test]
fn node_ping() {
   let alpha = node::Node::new().unwrap();
   let beta  = node::Node::new().unwrap();
   let beta_seed = beta.resources.local_info().address;
   let span = time::Duration::seconds(1);

   // Bootstrapping alpha:
   assert!(alpha.bootstrap(&beta_seed).is_ok());
   
   match alpha.state() {
      node::State::OffGrid => (), 
      _ => panic!("Should be off grid with this few nodes"),
   }

   let beta_receptions = beta.receptions()
      .during(span)
      .from(alpha.id().clone())
      .of_kind(receptions::KindFilter::Ping);

   // Alpha pings beta.
   assert!(alpha.resources.ping(&beta.local_info().address).is_ok());
   assert_eq!(1, beta_receptions.count());
}

#[test]
fn reception_iterator_times_out_correctly() {
   let alpha = node::Node::new().unwrap(); 
   let span = time::Duration::seconds(1);
   let maximum = time::Duration::seconds(3);
   let receptions = alpha.receptions().during(span);

   let before = time::SteadyTime::now(); 

   // nothing is happening, so this should time out in around a second (not necessarily precise)
   assert_eq!(0, receptions.count());

   let after = time::SteadyTime::now();

   assert!(after - before < maximum);
}

#[test]
fn bootstrapping_and_finding_on_simulated_network() {
   let mut nodes = simulated_network(30);

   // Head finds tail in a few steps.
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();

   assert_eq!(head.resources.locate(tail.id()).unwrap().id, tail.resources.local_info().id);
}

#[test]
fn finding_on_simulated_unresponsive_network() {

   let mut nodes = simulated_network(35);
   nodes.drain(10..20);
   assert_eq!(nodes.len(), 25);
   
   // Head finds tail in a few steps.
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();

   assert_eq!(head.resources.locate(tail.id()).unwrap().id, tail.resources.local_info().id);
}

#[test]
fn finding_a_nonexisting_node_in_a_simulated_network_times_out() {

   let mut nodes = simulated_network(30);
   
   // Head finds tail in a few steps.
   let head = nodes.pop_front().unwrap();

   let random_hash = hash::SubotaiHash::random();
   assert!(head.resources.locate(&random_hash).is_err());
}

fn simulated_network(network_size: usize) -> VecDeque<node::Node> {
   let cfg: node::Configuration = Default::default();
   assert!(network_size > cfg.k_factor, "You can't build a network with so few nodes!");

   let nodes: VecDeque<node::Node> = (0..network_size).map(|_| { node::Node::new().unwrap() }).collect();
   {
      let origin = nodes.front().unwrap();
      for node in nodes.iter().skip(1) {
         node.bootstrap(&origin.resources.local_info().address).unwrap();
      }
      for node in nodes.iter() {
         node.wait_for_state(node::State::OnGrid);
      }
   }
   nodes
}

#[test]
fn updating_table_with_full_bucket_starts_the_conflict_resolution_mechanism()
{
   let node = node::Node::new().unwrap();
   let cfg  = &node.resources.configuration;

   node.resources.table.fill_bucket(8, cfg.k_factor as u8); // Bucket completely full

   let mut id = node.id().clone();
   id.flip_bit(8);
   id.raw[0] = 0xFF;
   let info = node_info_no_net(id);

   node.resources.update_table(info);
   assert_eq!(node.resources.conflicts.lock().unwrap().len(), 1);
}

#[test]
fn generating_a_conflict_causes_a_ping_to_the_evicted_node()
{
   let alpha = node::Node::new().unwrap();
   let beta = node::Node::new().unwrap();
   let cfg  = &alpha.resources.configuration;
   alpha.resources.update_table(beta.resources.local_info());
  
   let index = alpha.resources.table.bucket_for_node(beta.id());

   // We fill the bucket corresponding to Beta until we are ready to cause a conflict.
   alpha.resources.table.fill_bucket(index, (cfg.k_factor -1) as u8);

   // We expect a ping to beta
   let pings = beta.receptions()
      .of_kind(receptions::KindFilter::Ping)
      .during(time::Duration::seconds(2));

   // Adding a new node causes a conflict.
   let mut id = beta.id().clone();
   id.raw[0] = 0xFF;
   let info = node_info_no_net(id);
   alpha.resources.update_table(info);
   
   assert_eq!(pings.count(), 1);
}

#[test]
fn generating_too_many_conflicts_causes_the_node_to_enter_defensive_state()
{
   let node = node::Node::new().unwrap();
   let cfg  = &node.resources.configuration;

   for index in 0..(cfg.k_factor + cfg.max_conflicts) {
      let mut id = node.id().clone();
      id.flip_bit(140); // Arbitrary bucket
      id.raw[0] = index as u8;
      let info = node_info_no_net(id);
      node.resources.update_table(info);
   }

   let state = *node.resources.state.read().unwrap();
   match state {
      node::State::Defensive => (),
      _ => panic!(),
   }

   // Trying to add new conflictive nodes while in defensive state will fail.
   let mut id = node.id().clone();
   id.flip_bit(140); // Arbitrary bucket
   id.raw[0] = 0xFF;
   let info = node_info_no_net(id.clone());

   node.resources.update_table(info);
   assert!(node.resources.table.specific_node(&id).is_none());

   // However, if they would fall in a different bucket, it's ok.
   id.flip_bit(155);
   let info = node_info_no_net(id.clone());
   node.resources.update_table(info);
   assert!(node.resources.table.specific_node(&id).is_some());
}

#[test]
fn node_probing_in_simulated_network()
{
   let cfg: node::Configuration = Default::default();
   let mut nodes = simulated_network(40);
   // We manually collect the info tags of all nodes.
   let mut info_nodes: Vec<routing::NodeInfo> = nodes
      .iter()
      .map(|ref node| node.resources.local_info())
      .collect();

   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();
   let probe_results = head.resources.probe(tail.id(), cfg.k_factor).unwrap();

   // We sort our manual collection by distance to the tail node.
   info_nodes.sort_by(|ref info_a, ref info_b| (&info_a.id ^ tail.id()).cmp(&(&info_b.id ^ tail.id())));
   info_nodes.truncate(cfg.k_factor); // This guarantees us the closest ids to the tail
   
   assert_eq!(info_nodes.len(), probe_results.len());

   for (a, b) in probe_results.iter().zip(info_nodes.iter()) {
      assert_eq!(a.id, b.id);
   }
}

#[test]
fn node_probing_in_simulated_unresponsive_network()
{
   let cfg: node::Configuration = Default::default();
   let mut nodes = simulated_network(40);
   // We manually collect the info tags of all nodes.
   let mut info_nodes: Vec<routing::NodeInfo> = nodes
      .iter()
      .map(|ref node| node.resources.local_info())
      .collect();

   nodes.drain(10..20);
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();
   let probe_results = head.resources.probe(tail.id(), cfg.k_factor).unwrap();

   // We sort our manual collection by distance to the tail node.
   info_nodes.sort_by(|ref info_a, ref info_b| (&info_a.id ^ tail.id()).cmp(&(&info_b.id ^ tail.id())));
   info_nodes.truncate(cfg.k_factor); // This guarantees us the closest ids to the tail
   
   assert_eq!(info_nodes.len(), probe_results.len());
   for (a, b) in probe_results.iter().zip(info_nodes.iter()) {
      assert_eq!(a.id, b.id);
   }
}

#[test]
fn bucket_pruning_removes_dead_nodes() {
   let mut nodes = simulated_network(40);
   let head = nodes.pop_front().unwrap();

   // let's find a bucket with nodes
   let index = (0..160).rev().find(|i| head.resources.table.nodes_from_bucket(*i).len() > 0).unwrap();
   let initial_nodes = head.resources.table.nodes_from_bucket(index).len();
   head.resources.prune_bucket(index).unwrap();
   assert_eq!(initial_nodes, head.resources.table.nodes_from_bucket(index).len());

   // Now when we kill the nodes, the pruning will work.
   nodes.clear();
   head.resources.prune_bucket(index).unwrap();

   assert_eq!(0, head.resources.table.nodes_from_bucket(index).len());
}

#[test]
fn store_retrieve_in_simulated_network()
{
   let mut nodes = simulated_network(40);
   let key = hash::SubotaiHash::random();
   let entry = storage::StorageEntry::Value(hash::SubotaiHash::random());
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();

   head.store(key.clone(), entry.clone()).unwrap();
   let retrieved_entries = tail.retrieve(&key).unwrap();
   assert_eq!(entry, retrieved_entries[0]);

   let key = hash::SubotaiHash::random();
   let blob: Vec<u8> = vec![0x00, 0x01, 0x02];
   let entry = storage::StorageEntry::Blob(blob.clone());

   head.store(key.clone(), entry.clone()).unwrap();
   let retrieved_entries = tail.retrieve(&key).unwrap();
   assert_eq!(entry, retrieved_entries[0]);

   // Now for mass storage
   let arbitrary_expiration = time::now() + time::Duration::minutes(30);
   let collection: Vec<_> = (0..10)
      .map(|_| (storage::StorageEntry::Value(hash::SubotaiHash::random()), arbitrary_expiration)).collect();
   let collection_key = hash::SubotaiHash::random();
   head.resources.mass_store(collection_key.clone(), collection.clone()).unwrap();
   let retrieved_collection = tail.retrieve(&collection_key).unwrap();
   let collection_entries: Vec<_> = collection.into_iter().map(|(entry, _)| entry).collect();
   assert_eq!(collection_entries.len(), retrieved_collection.len());
   assert_eq!(collection_entries, retrieved_collection);
}

fn node_info_no_net(id : hash::SubotaiHash) -> routing::NodeInfo {
   routing::NodeInfo {
      id : id,
      address : net::SocketAddr::from_str("0.0.0.0:0").unwrap(),
   }
}
