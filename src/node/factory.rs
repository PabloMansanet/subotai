//! #Factory
//!
//! The factory module allows you to create Subotai nodes with specific configuration options,
//! such as network constants and different UDP ports.
use {node, SubotaiResult};
use std::cmp;

/// Allows the construction of nodes with custom network constants, specific ports,
/// and other options.
pub struct Factory {
   configuration : node::Configuration,
   inbound_port  : u16,
   outbound_port : u16,
}

impl Factory {
   pub fn new() -> Self {
      Factory {
         configuration : Default::default(),
         inbound_port  : 0,
         outbound_port : 0,
      }
   }

   /// Creates a node with the configuration values specified in the factory. Defaults to the 
   /// same values as calling Node::new().
   pub fn create_node(&self) -> SubotaiResult<node::Node> {
      node::Node::with_configuration(self.inbound_port, self.outbound_port, self.configuration.clone())
   }
   
   /// Inbound UDP port for incoming RPCs.
   pub fn inbound_port(mut self, port: u16) -> Self {
      self.inbound_port = port;
      self
   }

   /// Outbound UDP port for outgoing RPCs.
   pub fn outbound_port(mut self, port: u16) -> Self {
      self.outbound_port = port;
      self
   }

   /// Network-wide concurrency factor. It's used, for example, to decide the
   /// number of remote nodes to interrogate concurrently when performing a 
   /// network-wide lookup.
   pub fn alpha(mut self, alpha: usize) -> Self {
      self.configuration.alpha = alpha;
      self.configuration.impatience = cmp::min(usize::saturating_sub(alpha, 1), self.configuration.impatience);
      self
   }

   /// Impatience factor, valid in the range [0..ALPHA). When performing "waves",
   /// the impatience factor denotes how many nodes we can give up waiting for, before
   /// starting the next wave. 
   ///
   /// If we send a request to ALPHA nodes during a lookup wave, we will start
   /// the next wave after we receive 'ALPHA - IMPATIENCE' responses.
   pub fn impatience(mut self, impatience: usize) -> Self {
      self.configuration.impatience = cmp::min(usize::saturating_sub(self.configuration.alpha, 1), impatience);;
      self
   }

   /// Data structure factor. It's used to dictate the size of the internal routing
   /// data structures (k-buckets).
   pub fn k_factor(mut self, k_factor: usize) -> Self {
      self.configuration.k_factor = k_factor;
      self
   }

   /// Maximum amount of eviction conflicts allowed before the node goes into
   /// a temporary defensive mode, and starts to prioritize old contacts to new, 
   /// potentially malicious ones.
   pub fn max_conflicts(mut self, max_conflicts: usize) -> Self {
      self.configuration.max_conflicts = max_conflicts;
      self
   }

   /// Maximum amount of storage entries (key-value or key-blob pairs). This has no
   /// effect on the routing table size (amount of node id-address pairs), which is
   /// dictated by the k_factor.
   pub fn max_storage(mut self, max_storage: usize) -> Self {
      self.configuration.max_storage = max_storage;
      self
   }

   /// Maximum size in bytes for a blob storage entry. (A blob entry consists in a 
   /// key associated with a chunk of binary data, instead of a 160 bit value hash).
   pub fn max_storage_blob_size(mut self, max_storage_blob_size: usize) -> Self {
      self.configuration.max_storage_blob_size = max_storage_blob_size;
      self
   }

   /// Xor distance from a key at which point nodes will start to dramatically decrease
   /// the expiration time for cached storage entries. This is only relevant in cases of 
   /// extreme network traffic around a given key. A bigger threshold allows for more
   /// over-caching and therefore less load on the critical nodes, while a smaller 
   /// threshold allows for a leaner network.
   pub fn expiration_distance_threshold(mut self, expiration_distance_threshold: usize) -> Self {
      self.configuration.expiration_distance_threshold = expiration_distance_threshold;
      self
   }

   /// Time in seconds after which it can be assumed that a remote node has failed to 
   /// respond to a query.
   pub fn network_timeout_s(mut self, network_timeout_s: i64) -> Self {
      self.configuration.network_timeout_s = network_timeout_s;
      self
   }

   /// Base expiration time for storage entries. Every time you call `store` on a node
   /// that resides on a live network (i.e. is in an `OnGrid` state) you guarantee the
   /// entry will remain in the network for this number of hours. Calling `store` again
   /// will refresh this time.
   pub fn base_expiration_time_hrs(mut self, base_expiration_time_hrs: i64) -> Self {
      self.configuration.base_expiration_time_hrs = base_expiration_time_hrs;
      self
   }
}

#[cfg(test)]
mod tests {
   use super::*;

   #[test]
   fn impatience_always_lower_than_alpha() {
      let factory = Factory::new().alpha(5).impatience(10);
      assert_eq!(factory.configuration.impatience, 4);
   }
}
