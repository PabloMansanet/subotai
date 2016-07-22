use node;
use time;
use std::collections::VecDeque;
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
fn chained_node_find() {
   let mut nodes: VecDeque<node::Node> = (0..10).map(|_| { node::Node::new().unwrap() }).collect();

   // We inform each node of the existance of the next three.
   for i in 1..4 {
      for (alpha, beta) in nodes.iter().zip(nodes.iter().skip(i)) {
         alpha.resources.table.insert_node(beta.local_info());    
      }
   }

   // Head finds tail
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();
   assert_eq!(head.find_node(tail.id()).unwrap().id, tail.local_info().id);
}

#[test]
fn bootstrapping_and_finding_on_simulated_network() {
   let origin = node::Node::new().unwrap();

   let mut nodes: VecDeque<node::Node> = (0..100).map(|_| { node::Node::new().unwrap() }).collect();

   for node in nodes.iter() {
      assert!(node.bootstrap(origin.local_info()).is_ok());
   }
   
   // Head finds tail in a few steps.
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();

   let receptions = 
      head.receptions()
          .during(time::Duration::seconds(1))
          .rpc(receptions::RpcFilter::FindNodeResponse);

   assert_eq!(head.find_node(tail.id()).unwrap().id, tail.local_info().id);
   let maximum_responses = 10;
   let responses = receptions.rpc(receptions::RpcFilter::FindNodeResponse).count();
   assert!(responses < maximum_responses);
}

