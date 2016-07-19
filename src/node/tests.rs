use node;
use time;
use std::collections::VecDeque;
use node::receptions;

pub const POLL_FREQUENCY_MS: u64 = 50;
pub const TRIES: u8 = 5;

#[test]
fn node_ping() {
   let alpha = node::Node::new();
   let beta  = node::Node::new();
   let beta_seed = beta.local_info();
   let span = time::Duration::seconds(1);

   // Bootstrapping alpha:
   alpha.bootstrap(beta_seed);

   let beta_receptions = alpha.receptions().during(span).from(beta.id().clone());

   // Alpha pings beta.
   assert_eq!(alpha.ping(beta.resources.id.clone()), node::PingResult::Alive);
   assert_eq!(1, beta_receptions.count());
}

#[test]
fn reception_iterator_times_out_correctly() {
   let alpha = node::Node::new(); 
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
   let mut nodes: VecDeque<node::Node> = (0..10).map(|_| { node::Node::new() }).collect();

   // We inform each node of the existance of the next three.
   for i in 1..4 {
      for (alpha, beta) in nodes.iter().zip(nodes.iter().skip(i)) {
         alpha.resources.table.insert_node(beta.local_info());    
      }
   }

   // Head finds tail
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();
   assert_eq!(head.find_node(tail.id()).unwrap(), tail.local_info());
}

#[test]
fn bootstrapping_and_finding_on_simulated_network() {
   let origin = node::Node::new();

   let mut nodes: VecDeque<node::Node> = (0..100).map(|_| { node::Node::new() }).collect();

   for node in nodes.iter() {
      node.bootstrap(origin.local_info());
   }
   
   // Head finds tail in a few steps.
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();

   let receptions = 
      head.receptions()
          .during(time::Duration::seconds(3))
          .rpc(receptions::RpcFilter::FindNodeResponse);

   assert_eq!(head.find_node(tail.id()).unwrap(), tail.local_info());
   let maximum_steps = 10;
   assert!(receptions.count() < maximum_steps);
}
