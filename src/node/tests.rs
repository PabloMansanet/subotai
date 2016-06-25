use node;
use rpc;
use std::thread;
use std::time;
use routing;
use std::net;
use std::str::FromStr;
pub const POLL_FREQUENCY_MS: u64 = 50;
pub const TRIES: u8 = 5;

#[test]
fn node_ping() {
   let alpha = node::Node::new(50000, 50001).unwrap();
   let beta  = node::Node::new(50002, 50003).unwrap();

   let ip = net::IpAddr::from_str("127.0.0.1").unwrap();
   let address_beta = net::SocketAddr::new(ip, 50002);

   let info_beta = routing::NodeInfo { 
      node_id : beta.resources.id.clone(),
      address : address_beta,
   };

   // Before sending the ping, beta does not know about alpha.
   assert!(beta.resources.table.specific_node(&alpha.resources.id).is_none());

   // Alpha pings beta.
   alpha.ping(info_beta);

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
   let alpha = node::Node::new(50004, 50005).unwrap();
   let beta  = node::Node::new(50006, 50007).unwrap();

   let ip = net::IpAddr::from_str("127.0.0.1").unwrap();
   let address_beta = net::SocketAddr::new(ip, 50006);

   let info_beta = routing::NodeInfo { 
      node_id : beta.resources.id.clone(),
      address : address_beta,
   };

   alpha.ping(info_beta);
   for reply in alpha.receptions().take(1) { 
      assert_eq!(rpc::Kind::PingResponse, reply.kind);
   }
}
