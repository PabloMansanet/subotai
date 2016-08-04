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
   let beta_seed = beta.local_info();
   let span = time::Duration::seconds(1);

   // Bootstrapping alpha:
   assert!(alpha.bootstrap_until(beta_seed, 1).is_ok());

   let beta_receptions = beta.receptions()
      .during(span)
      .from(alpha.id().clone())
      .of_kind(receptions::KindFilter::Ping);

   // Alpha pings beta.
   assert!(alpha.ping(&beta.resources.id).is_ok());
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

   assert_eq!(head.find_node(tail.id()).unwrap().id, tail.local_info().id);
}

#[test]
fn finding_on_simulated_unresponsive_network() {

   let mut nodes = simulated_network(30);
   nodes.drain(10..20);
   assert_eq!(nodes.len(), 20);
   
   // Head finds tail in a few steps.
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();

   assert_eq!(head.find_node(tail.id()).unwrap().id, tail.local_info().id);
}

#[test]
#[ignore]
fn finding_a_nonexisting_node_in_a_simulated_network_times_out() {

   let mut nodes = simulated_network(30);
   
   // Head finds tail in a few steps.
   let head = nodes.pop_front().unwrap();

   let random_hash = hash::SubotaiHash::random();
   assert!(head.find_node(&random_hash).is_err());
}

fn simulated_network(nodes: usize) -> VecDeque<node::Node> {
   let nodes: VecDeque<node::Node> = (0..nodes).map(|_| { node::Node::new().unwrap() }).collect();
   {
      let origin = nodes.front().unwrap();
      // Initial handshake pass
      for node in nodes.iter().skip(1) {
         assert!(node.bootstrap_until(origin.local_info(), 1).is_ok());
      }

      // Actual bootstrapping
      for node in nodes.iter().skip(1) {
         assert!(node.bootstrap(origin.local_info()).is_ok());
      }
   }
   nodes
}

#[test]
fn updating_table_with_full_bucket_starts_the_conflict_resolution_mechanism()
{
   let node = node::Node::new().unwrap();
   node.resources.table.fill_bucket(8, routing::K_FACTOR as u8); // Bucket completely full

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
   alpha.resources.update_table(beta.local_info());
  
   let index = alpha.resources.table.bucket_for_node(beta.id());

   // We fill the bucket corresponding to Beta until we are ready to cause a conflict.
   alpha.resources.table.fill_bucket(index, (routing::K_FACTOR -1) as u8);

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

   for index in 0..(routing::K_FACTOR + routing::MAX_CONFLICTS) {
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
fn remote_key_value_storage()
{
   let alpha = node::Node::new().unwrap();
   let beta  = node::Node::new().unwrap();
   
   assert!(alpha.bootstrap_until(beta.local_info(), 1).is_ok());
  
   let key = hash::SubotaiHash::random();
   let value = hash::SubotaiHash::random();
   let mut beta_receptions = 
      beta.receptions()
          .of_kind(receptions::KindFilter::Store)
          .during(time::Duration::seconds(1));

   let mut alpha_receptions =
      alpha.receptions()
           .of_kind(receptions::KindFilter::StoreResponse)
           .during(time::Duration::seconds(1));

   assert_eq!(alpha.resources.store_remotely(&beta.local_info(), key.clone(), value.clone()).unwrap(),
              storage::StoreResult::Success);
   assert!(alpha_receptions.next().is_some());
   assert!(beta_receptions.next().is_some());
   
   let retrieved_value = beta.resources.storage.get(&key).unwrap();
   assert_eq!(value, retrieved_value);
}

#[test]
fn node_probing_in_simulated_network()
{
   let mut nodes = simulated_network(40);
   // We manually collect the info tags of all nodes.
   let mut info_nodes: Vec<routing::NodeInfo> = nodes
      .iter()
      .map(|ref node| node.local_info())
      .collect();

   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();
   let probe_results = head.resources.probe(tail.id()).unwrap();

   // We sort our manual collection by distance to the tail node.
   info_nodes.sort_by(|ref info_a, ref info_b| (&info_a.id ^ tail.id()).cmp(&(&info_b.id ^ tail.id())));
   info_nodes.truncate(routing::K_FACTOR); // This guarantees us the closest ids to the tail
   
   assert_eq!(info_nodes.len(), probe_results.len());
   for (a, b) in probe_results.iter().zip(info_nodes.iter()) {
      assert_eq!(a.id, b.id);
   }
}

#[test]
fn node_probing_in_simulated_unresponsive_network()
{
   let mut nodes = simulated_network(40);
   // We manually collect the info tags of all nodes.
   let mut info_nodes: Vec<routing::NodeInfo> = nodes
      .iter()
      .map(|ref node| node.local_info())
      .collect();

   nodes.drain(10..20);
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();
   let probe_results = head.resources.probe(tail.id()).unwrap();

   // We sort our manual collection by distance to the tail node.
   info_nodes.sort_by(|ref info_a, ref info_b| (&info_a.id ^ tail.id()).cmp(&(&info_b.id ^ tail.id())));
   info_nodes.truncate(routing::K_FACTOR); // This guarantees us the closest ids to the tail
   
   assert_eq!(info_nodes.len(), probe_results.len());
   for (a, b) in probe_results.iter().zip(info_nodes.iter()) {
      assert_eq!(a.id, b.id);
   }
}

#[test]
fn store_retrieve_in_simulated_network()
{
   let mut nodes = simulated_network(40);
   let (key,value) = (hash::SubotaiHash::random(), hash::SubotaiHash::random());
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();

   head.store(key.clone(), value.clone()).unwrap();
   let retrieved_value = tail.retrieve(&key).unwrap();;

   assert_eq!(value, retrieved_value);
}

fn node_info_no_net(id : hash::SubotaiHash) -> routing::NodeInfo {
   routing::NodeInfo {
      id : id,
      address : net::SocketAddr::from_str("0.0.0.0:0").unwrap(),
   }
}
