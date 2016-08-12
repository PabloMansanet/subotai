#[derive(Clone, Debug)]
pub struct Configuration {
   pub alpha                         : usize,
   pub impatience                    : usize,
   pub k_factor                      : usize,
   pub max_conflicts                 : usize,
   pub max_storage                   : usize,
   pub hash_size_bytes               : usize,
   pub expiration_distance_threshold : usize,
   pub network_timeout_s             : i64,
   pub base_expiration_time_hrs      : i64,
}

impl Default for Configuration {
   fn default() -> Configuration {
      Configuration {
         alpha                         : 3,
         impatience                    : 1,
         k_factor                      : 20,
         max_conflicts                 : 60,
         max_storage                   : 10000,
         hash_size_bytes               : 20,
         expiration_distance_threshold : 8,
         network_timeout_s             : 5,
         base_expiration_time_hrs      : 24,
      }
   }
}

pub struct Factory {
   configuration: Configuration,
}
