use node;
use rpc;

use std::thread;
use std::time;
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

   // Eventually, beta knows of alpha.
   let mut found = beta.resources.table.specific_node(&alpha.resources.id);
   for _ in 0..TRIES {
      if found.is_some() {
         break;
      }
      found = beta.resources.table.specific_node(&alpha.resources.id);
      thread::sleep(time::Duration::from_millis(POLL_FREQUENCY_MS));
   }

   assert!(found.is_some());
}

#[test]
fn ping_response() {
   let alpha = node::Node::new();
   let beta  = node::Node::new();
   let beta_seed = beta.local_info();

   // Bootstrapping alpha:
   alpha.bootstrap(beta_seed);

   alpha.ping(beta.resources.id.clone());
   for reply in alpha.receptions().take(1) { 
      assert_eq!(rpc::Kind::PingResponse, reply.kind);
   }
}
