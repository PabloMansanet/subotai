use {node, routing, time, hash};
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

   let beta_receptions = alpha.receptions().during(span).from(beta.id().clone());

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
   nodes.drain(5..25);
   assert_eq!(nodes.len(), 10);
   
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

   let random_hash = hash::Hash::random();
   assert!(head.find_node(&random_hash).is_err());
}

fn simulated_network(nodes: usize) -> VecDeque<node::Node> {
   let nodes: VecDeque<node::Node> = (0..nodes).map(|_| { node::Node::new().unwrap() }).collect();
   {
      let origin = node::Node::new().unwrap();

      // Initial handshake pass
      for node in nodes.iter() {
         assert!(node.bootstrap_until(origin.local_info(), 1).is_ok());
      }

      // Actual bootstrapping
      for node in nodes.iter() {
         assert!(node.bootstrap(origin.local_info()).is_ok());
      }
   }
   nodes
}

#[test]
fn updating_table_with_full_bucket_starts_the_conflict_resolution_mechanism()
{
   let node = node::Node::new().unwrap();
   node.resources.table.fill_bucket(8, routing::K as u8); // Bucket completely full

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
  
   let index = alpha.resources.table.bucket_for_node(beta.id()).unwrap();

   // We fill the bucket corresponding to Beta until we are ready to cause a conflict.
   alpha.resources.table.fill_bucket(index, (routing::K -1) as u8);

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


fn node_info_no_net(id : hash::Hash) -> routing::NodeInfo {
   routing::NodeInfo {
      id : id,
      address : net::SocketAddr::from_str("0.0.0.0:0").unwrap(),
   }
}
