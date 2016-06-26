use routing;
use rpc;
use bus;
use rpc::Rpc;
use time;

use hash::Hash;
use bincode::serde;
use std::net;
use std::sync;
use node::*;

/// Node resources for synchronous operations.
///
/// All methods on this module are synchronous, and will wait for any
/// remote nodes queried to reply to the RPCs sent, up to the timeout
/// defined at `node::NETWORK_TIMEOUT_S`. The node layer above is in 
/// charge of parallelizing those operations by spawning threads when
/// adequate.
pub struct Resources {
   pub id       : Hash,
   pub table    : routing::Table,
   pub outbound : net::UdpSocket,
   pub inbound  : net::UdpSocket,
   pub state    : sync::Mutex<State>,
   pub updates  : sync::Mutex<bus::Bus<Update>>,
}

#[derive(Clone)]
pub enum Update {
   RpcReceived(Rpc),
   Tick,
}

impl Resources {
   pub fn ping(&self, id: Hash) {
      if let Some(destination) = self.table.specific_node(&id)
      {
         let rpc = Rpc::ping(self.id.clone(), self.inbound.local_addr().unwrap().port());
         let packet = rpc.serialize();
         self.outbound.send_to(&packet, destination.address);
      }
   }

   pub fn ping_response(&self, destination: routing::NodeInfo) {
      let rpc = Rpc::ping_response(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let packet = rpc.serialize();
      self.outbound.send_to(&packet, destination.address);
   }

   pub fn receptions(&self) -> receptions::Receptions {
      receptions::Receptions::new(self)
   }

   /// Attempts to find a node through the network.
   pub fn find_node(&self, id_to_find: Hash) -> Option<routing::NodeInfo> {
      let mut queried_nodes = Vec::<Hash>::with_capacity(routing::K);

      while queried_nodes.len() < routing::K {
         match self.table.lookup(&id_to_find, routing::ALPHA, Some(&queried_nodes)) {
            routing::LookupResult::Found(node) => return Some(node),
            routing::LookupResult::ClosestNodes(nodes) => self.lookup_wave(&id_to_find, &nodes, &mut queried_nodes),
            routing::LookupResult::Myself => break,
            routing::LookupResult::Nothing => break,
         }
      }
      None
   }

   fn lookup_wave(&self, id_to_find: &Hash, nodes_to_query: &Vec<routing::NodeInfo>, queried: &mut Vec<Hash>) {
      for node in nodes_to_query {
         let rpc = Rpc::find_node(
            self.id.clone(), 
            self.inbound.local_addr().unwrap().port(),
            id_to_find.clone(),
            routing::ALPHA,
         );
         let packet = rpc.serialize(); 
         self.outbound.send_to(&packet, node.address);
         queried.push(node.id.clone());
      }

      for reception in self.receptions()
         .during(time::Duration::seconds(NETWORK_TIMEOUT_S))
         .filter(|rpc: &Rpc| {
             match rpc {
                &Rpc{ kind: rpc::Kind::FindNodeResponse(box ref id_to_find), ..} => true,
                _ => false,
             }
         })
         .take(routing::ALPHA){}
   }

   pub fn process_incoming_rpc(&self, rpc: Rpc, mut source: net::SocketAddr) -> serde::DeserializeResult<()> {
      source.set_port(rpc.reply_port);
      let sender = routing::NodeInfo {
         id      : rpc.sender_id.clone(),
         address : source,
      };

      match rpc.kind {
         rpc::Kind::Ping         => self.handle_ping(rpc.clone(), sender),
         rpc::Kind::PingResponse => self.handle_ping_response(rpc.clone(), sender),
         _ => (),
      }

      self.updates.lock().unwrap().broadcast(Update::RpcReceived(rpc));
      Ok(())
   }

   pub fn handle_ping(&self, ping: Rpc, sender: routing::NodeInfo) {
      self.table.insert_node(sender.clone());
      self.ping_response(sender);
   }

   fn handle_ping_response(&self, ping: Rpc, sender: routing::NodeInfo) {
      self.table.insert_node(sender);
   }
}

