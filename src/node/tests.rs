use node;
use rpc;

use std::thread;
use time;
use std::time::Duration as StdDuration;
use std::net;
use std::str::FromStr;

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
   alpha.ping(beta.resources.id.clone());

   // Beta responds
   for reply in alpha.receptions()
                     .during(time::Duration::seconds(1))
                     .take(1) { 
      assert_eq!(rpc::Kind::PingResponse, reply.kind);
   }

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
