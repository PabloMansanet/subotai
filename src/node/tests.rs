use node;
use time;
use std::collections::VecDeque;

pub const POLL_FREQUENCY_MS: u64 = 50;
pub const TRIES: u8 = 5;

#[test]
fn node_ping() {
   let alpha = node::Node::new().unwrap();
   let beta  = node::Node::new().unwrap();
   let beta_seed = beta.local_info();
   let span = time::Duration::seconds(1);

   // Bootstrapping alpha:
   assert!(alpha.bootstrap(beta_seed).is_ok());

   let beta_receptions = alpha.receptions().during(span).from(beta.id().clone());

   // Alpha pings beta.
   assert!(alpha.ping(beta.resources.id.clone()).is_ok());
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

   let mut nodes: VecDeque<node::Node> = (0..100).map(|_| { node::Node::new().unwrap() }).collect();
   {
      let origin = node::Node::new().unwrap();
      for node in nodes.iter() {
         assert!(node.bootstrap(origin.local_info()).is_ok());
      }
   }
   
   // Head finds tail in a few steps.
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();

   assert_eq!(head.find_node(tail.id()).unwrap().id, tail.local_info().id);
}

#[test]
fn finding_on_simulated_unresponsive_network() {

   let mut nodes: VecDeque<node::Node> = (0..100).map(|_| { node::Node::new().unwrap() }).collect();

   {
      let origin = node::Node::new().unwrap();
      for node in nodes.iter() {
         assert!(node.bootstrap(origin.local_info()).is_ok());
      }
   }

   nodes.drain(30..70);
   assert_eq!(nodes.len(), 60);
   
   // Head finds tail in a few steps.
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();

   assert_eq!(head.find_node(tail.id()).unwrap().id, tail.local_info().id);
}

