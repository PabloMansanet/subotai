use routing;
use rpc;
use bus;

use hash::Hash;
use bincode::serde;
use std::{net, io, thread, time};
use std::sync;
use std::sync::{Weak, Arc};
use node::*;

pub struct Resources {
   pub id       : Hash,
   pub table    : routing::Table,
   pub outbound : net::UdpSocket,
   pub inbound  : net::UdpSocket,
   pub state    : sync::Mutex<State>,
   pub received : sync::Mutex<bus::Bus<rpc::Rpc>>,
}

impl Resources {
   pub fn ping(&self, destination: routing::NodeInfo) {
      let ping = rpc::Rpc::ping(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let payload = ping.serialize();
      self.outbound.send_to(&payload, destination.address);
   }

   pub fn ping_response(&self, destination: routing::NodeInfo) {
      let ping_response = rpc::Rpc::ping_response(self.id.clone(), self.inbound.local_addr().unwrap().port());
      let payload = ping_response.serialize();
      self.outbound.send_to(&payload, destination.address);
   }

   pub fn process_incoming_rpc(&self, rpc: rpc::Rpc, mut source: net::SocketAddr) -> serde::DeserializeResult<()> {
      source.set_port(rpc.reply_port);
      let sender = routing::NodeInfo {
         node_id : rpc.sender_id.clone(),
         address : source,
      };

      match rpc.kind {
         rpc::Kind::Ping         => self.handle_ping(rpc.clone(), sender),
         rpc::Kind::PingResponse => self.handle_ping_response(rpc.clone(), sender),
         _ => (),
      }

      self.received.lock().unwrap().broadcast(rpc);
      Ok(())
   }

   pub fn handle_ping(&self, ping: rpc::Rpc, sender: routing::NodeInfo) {
      self.table.insert_node(sender.clone());
      self.ping_response(sender);
   }

   fn handle_ping_response(&self, ping: rpc::Rpc, sender: routing::NodeInfo) {
      self.table.insert_node(sender);
   }
}

