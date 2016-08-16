use {node, SubotaiResult};
use std::cmp;

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

   pub fn create_node(&self) -> SubotaiResult<node::Node> {
      node::Node::with_ports_and_configuration(self.inbound_port, self.outbound_port, self.configuration.clone())
   }

   pub fn inbound_port(mut self, port: u16) -> Self {
      self.inbound_port = port;
      self
   }

   pub fn outbound_port(mut self, port: u16) -> Self {
      self.outbound_port = port;
      self
   }

   pub fn alpha(mut self, alpha: usize) -> Self {
      self.configuration.alpha = alpha;
      self.configuration.impatience = cmp::min(usize::saturating_sub(alpha, 1), self.configuration.impatience);
      self
   }

   pub fn impatience(mut self, impatience: usize) -> Self {
      self.configuration.impatience = cmp::min(usize::saturating_sub(self.configuration.alpha, 1), impatience);;
      self
   }

   pub fn k_factor(mut self, k_factor: usize) -> Self {
      self.configuration.k_factor = k_factor;
      self
   }

   pub fn max_conflicts(mut self, max_conflicts: usize) -> Self {
      self.configuration.max_conflicts = max_conflicts;
      self
   }

   pub fn max_storage(mut self, max_storage: usize) -> Self {
      self.configuration.max_storage = max_storage;
      self
   }

   pub fn max_storage_blob_size(mut self, max_storage_blob_size: usize) -> Self {
      self.configuration.max_storage_blob_size = max_storage_blob_size;
      self
   }

   pub fn expiration_distance_threshold(mut self, expiration_distance_threshold: usize) -> Self {
      self.configuration.expiration_distance_threshold = expiration_distance_threshold;
      self
   }

   pub fn network_timeout_s(mut self, network_timeout_s: i64) -> Self {
      self.configuration.network_timeout_s = network_timeout_s;
      self
   }

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
