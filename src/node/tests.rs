use node;
use time;
use std::collections::VecDeque;

pub const POLL_FREQUENCY_MS: u64 = 50;
pub const TRIES: u8 = 5;

#[test]
fn node_ping() {
   let alpha = node::Node::new();
   let beta  = node::Node::new();
   let beta_seed = beta.local_info();

   // Bootstrapping alpha:
   alpha.bootstrap(beta_seed);

   // Before sending the ping, beta does not know about alpha.
   assert!(beta.resources.table.specific_node(&alpha.resources.id).is_none());

   // Alpha pings beta.
   assert_eq!(alpha.ping(beta.resources.id.clone()), node::PingResult::Alive);

   // And beta now knows of alpha
   assert!(beta.resources.table.specific_node(&alpha.resources.id).is_some());
}

#[test]
fn reception_iterator_times_out_correctly() {
   let alpha = node::Node::new(); 
   let span = time::Duration::seconds(1);
   let maximum = time::Duration::seconds(3);
   let receptions = alpha.receptions().during(span);

   let before = time::SteadyTime::now(); 
   for rpc in receptions {
      // nothing is happening, so this should time out in around a second (not necessarily precise)
      panic!();
   }
   let after = time::SteadyTime::now();

   assert!(after - before < maximum);
}

#[test]
fn chained_node_find() {
   let mut nodes: VecDeque<node::Node> = (0..10).map(|_| { node::Node::new() }).collect();

   // We inform each node of the existance of the next two.
   for ((alpha, beta), gamma) in nodes.iter()
                                      .zip(nodes.iter().skip(1))
                                      .zip(nodes.iter().skip(2)) 
   {
      alpha.resources.table.insert_node(beta.local_info());    
      alpha.resources.table.insert_node(gamma.local_info());    
   }

   // Head finds tail
   let head = nodes.pop_front().unwrap();
   let tail = nodes.pop_back().unwrap();
   assert_eq!(head.find_node(tail.id()).unwrap(), tail.local_info());
}
