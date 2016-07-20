use routing;
use rpc;
use bus;

use rpc::Rpc;
use time;
use {SubotaiError, SubotaiResult};

use hash::Hash;
use std::net;
use std::sync::Arc;
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
   pub fn local_info(&self) -> routing::NodeInfo {
      routing::NodeInfo {
         id      : self.id.clone(),
         address : self.inbound.local_addr().unwrap(),
      }
   }

   pub fn ping(&self, id: Hash) -> SubotaiResult<()> {
      let node = match self.table.specific_node(&id) {
         None => Some(try!(self.find_node(&id))),
         Some(node) => Some(node),
      };

      if let Some(node) = node {
         let rpc = Rpc::ping(self.id.clone(), self.inbound.local_addr().unwrap().port());
         let packet = rpc.serialize();
         let responses = self.receptions().during(time::Duration::seconds(NETWORK_TIMEOUT_S))
            .rpc(receptions::RpcFilter::PingResponse).from(id.clone()).take(1);
         
         try!(self.outbound.send_to(&packet, node.address));

         for _ in responses {
            return Ok(());
         }
      }
      Err(SubotaiError::NoResponse)
   }

   pub fn receptions(&self) -> receptions::Receptions {
      receptions::Receptions::new(self)
   }

   /// Attempts to find a node through the network.
   pub fn find_node(&self, id_to_find: &Hash) -> SubotaiResult<routing::NodeInfo> {
      let mut queried_nodes = Vec::<Hash>::with_capacity(routing::K);

      while queried_nodes.len() < routing::K {
         match self.table.lookup(id_to_find, routing::ALPHA, Some(&queried_nodes)) {
            routing::LookupResult::Found(node) => return Ok(node),
            routing::LookupResult::ClosestNodes(nodes) => {
               try!(self.lookup_wave(id_to_find, &nodes, &mut queried_nodes));
            },
            _ => break,
         }
      }
     
      match self.table.specific_node(id_to_find) {
         Some(node) => Ok(node),
         None => Err(SubotaiError::NodeNotFound),
      }
   }

   pub fn bootstrap(&self, seed: routing::NodeInfo) -> SubotaiResult<()>  {
      self.table.insert_node(seed);
      let mut queried_ids = Vec::<Hash>::with_capacity(routing::K);

      while queried_ids.len() < routing::K {
         let unqueried_nodes: Vec<routing::NodeInfo> = self.table.all_nodes()
            .filter(|ref node_info| !queried_ids.contains(&node_info.id))
            .collect();

         if unqueried_nodes.is_empty() {
            break;
         }

         try!(self.bootstrap_wave(&unqueried_nodes, &mut queried_ids));
      }

      if self.table.len() <= 1 {
         Err(SubotaiError::NoResponse)
      } else {
         Ok(())
      }
   }

   fn bootstrap_wave(&self, nodes_to_query: &[routing::NodeInfo], queried: &mut Vec<Hash>) -> SubotaiResult<()> {
      let responses = self.receptions()
         .during(time::Duration::seconds(NETWORK_TIMEOUT_S))
         .rpc(receptions::RpcFilter::BootstrapResponse)
         .take(nodes_to_query.len());

      for node in nodes_to_query {
         let rpc = Rpc::bootstrap(self.id.clone(), self.inbound.local_addr().unwrap().port());
         let packet = rpc.serialize(); 
         try!(self.outbound.send_to(&packet, node.address));
         queried.push(node.id.clone());
      }

      responses.count();
      Ok(())
   }

   fn lookup_wave(&self, id_to_find: &Hash, nodes_to_query: &Vec<routing::NodeInfo>, queried: &mut Vec<Hash>) -> SubotaiResult<()> {
      let responses = self.receptions()
         .during(time::Duration::seconds(NETWORK_TIMEOUT_S))
         .filter(|rpc: &Rpc| {
             match rpc.kind {
                rpc::Kind::FindNodeResponse( ref payload ) => &payload.id_to_find == id_to_find,
                _ => false,
             }
         }).take(nodes_to_query.len());

      for node in nodes_to_query {
         let rpc = Rpc::find_node(
            self.id.clone(), 
            self.inbound.local_addr().unwrap().port(),
            id_to_find.clone(),
            routing::ALPHA,
         );
         let packet = rpc.serialize(); 
         try!(self.outbound.send_to(&packet, node.address));
         queried.push(node.id.clone());
      }

      for response in responses { 
         if let rpc::Kind::FindNodeResponse(ref payload) = response.kind {
            match payload.result {
               routing::LookupResult::Found(_) => return Ok(()),
               routing::LookupResult::Myself   => return Ok(()),
               _ => (),
            }
         }
      }

      Ok(())
   }

   pub fn process_incoming_rpc(&self, rpc: Rpc, mut source: net::SocketAddr) -> SubotaiResult<()>{
      source.set_port(rpc.reply_port);
      let sender = routing::NodeInfo {
         id      : rpc.sender_id.clone(),
         address : source,
      };

      let result = match rpc.kind {
         rpc::Kind::Ping                           => self.handle_ping(sender),
         rpc::Kind::PingResponse                   => self.handle_ping_response(sender),
         rpc::Kind::FindNode(ref payload)          => self.handle_find_node(payload.clone(), sender),
         rpc::Kind::FindNodeResponse(ref payload)  => self.handle_find_node_response(payload.clone(), sender),
         rpc::Kind::Bootstrap                      => self.handle_bootstrap(sender),
         rpc::Kind::BootstrapResponse(ref payload) => self.handle_bootstrap_response(payload.clone(),sender),
         _ => unimplemented!(),
      };

      self.updates.lock().unwrap().broadcast(Update::RpcReceived(rpc));
      result
   }

   fn handle_ping(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.insert_node(sender.clone());
      let rpc = Rpc::ping_response(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_bootstrap(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.insert_node(sender.clone());
      let closest_to_sender: Vec<_> = self.table.closest_nodes_to(&sender.id)
         .filter(|ref info| &info.id != &sender.id) // We don't want to reply with the sender itself
         .take(routing::ALPHA)
         .collect();

      let rpc = Rpc::bootstrap_response(self.id.clone(), self.inbound.local_addr().unwrap().port(), closest_to_sender);
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_bootstrap_response(&self, payload: Arc<rpc::BootstrapResponsePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.insert_node(sender.clone());
      for node in &payload.nodes {
         self.table.insert_node(node.clone());
      }
      Ok(())
   }

   fn handle_ping_response(&self, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.insert_node(sender);
      Ok(())
   }

   fn handle_find_node(&self, payload: Arc<rpc::FindNodePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.insert_node(sender.clone());
      let lookup_results = self.table.lookup(&payload.id_to_find, payload.nodes_wanted, None);
      let rpc = Rpc::find_node_response(self.id.clone(), 
                                        self.inbound.local_addr().unwrap().port(),
                                        payload.id_to_find.clone(),
                                        lookup_results);
      let packet = rpc.serialize();
      try!(self.outbound.send_to(&packet, sender.address));
      Ok(())
   }

   fn handle_find_node_response(&self, payload: Arc<rpc::FindNodeResponsePayload>, sender: routing::NodeInfo) -> SubotaiResult<()> {
      self.table.insert_node(sender);
      match payload.result {
         routing::LookupResult::ClosestNodes(ref nodes) => for node in nodes { self.table.insert_node(node.clone()) },
         routing::LookupResult::Found(ref node) => self.table.insert_node(node.clone()),
         _ => (),
      }
      Ok(())
   }
}

