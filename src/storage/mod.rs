use {time, node};
use hash::SubotaiHash;
use std::collections::HashMap;
use std::sync::RwLock;
use std::cmp;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageEntry {
   Value(SubotaiHash),
   Blob(Vec<u8>),
}

#[derive(Debug, Clone)]
struct EntryAndTimes {
   entry           : StorageEntry,
   expiration      : time::SteadyTime,
   republish_ready : bool,
}

pub struct Storage {
   entries_and_times : RwLock<HashMap<SubotaiHash, EntryAndTimes> >,
   parent_id         : SubotaiHash,
   configuration     : node::Configuration,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum StoreResult {
   Success,
   StorageFull,
   BlobTooBig,
}

impl Storage {
   pub fn new(parent_id: SubotaiHash, configuration: node::Configuration) -> Storage {
      Storage {
         entries_and_times : RwLock::new(HashMap::with_capacity(configuration.max_storage)),
         parent_id         : parent_id,
         configuration     : configuration,
      }
   }
   
   pub fn len(&self) -> usize {
      self.entries_and_times.read().unwrap().len()
   }

   pub fn is_empty(&self) -> bool {
      self.entries_and_times.read().unwrap().is_empty()
   }

   pub fn store(&self, key: SubotaiHash, entry: StorageEntry) -> StoreResult {
      let mut entries_and_times = self.entries_and_times.write().unwrap();

      // If the key was already present, the expiration time is kept.
      let expiration = if let Some(old) = entries_and_times.remove(&key) {
         old.expiration
      } else {
         self.calculate_expiration_date(&key)
      };

      let entry_and_times = EntryAndTimes { entry: entry, expiration: expiration, republish_ready: false };

      if entries_and_times.len() >= self.configuration.max_storage {
         StoreResult::StorageFull
      } else if self.is_big_blob(&entry_and_times.entry){
         StoreResult::BlobTooBig
      } else {
         entries_and_times.insert(key.clone(), entry_and_times);
         StoreResult::Success
      }
   }

   fn is_big_blob(&self, entry: &StorageEntry) -> bool {
      match entry {
         &StorageEntry::Blob(ref vec) => vec.len() > self.configuration.max_storage_blob_size,
         _ => false,
      }
   }

   /// Retrieves a particular entry given a key.
   pub fn retrieve(&self, key: &SubotaiHash) -> Option<StorageEntry> {
      if let Some( &EntryAndTimes { ref entry, .. } ) = self.entries_and_times.read().unwrap().get(key) {
         Some(entry.clone())
      } else {
         None
      }
   }

   pub fn clear_expired_entries(&self) {
      let now = time::SteadyTime::now();
      let mut entries_and_times = self.entries_and_times.write().unwrap();
      let keys_to_remove: Vec<_>= entries_and_times
         .iter()
         .filter_map(|(key, &EntryAndTimes{ expiration, .. })| if expiration < now { Some(key) } else { None })
         .cloned()
         .collect();

      for key in keys_to_remove {
         entries_and_times.remove(&key);
      }
   }

   /// Marks all key-entry pairs as ready for republishing.
   pub fn mark_all_as_ready(&self) {
      for (_, &mut EntryAndTimes {ref mut republish_ready, ..})  in self.entries_and_times.write().unwrap().iter_mut() {
         *republish_ready = true;
      }
   }

   /// Marks a particular key-entry pair as not ready for republishing.
   pub fn mark_as_not_ready(&self, key: &SubotaiHash) {
      if let Some( &mut EntryAndTimes {ref mut republish_ready, ..}) = self.entries_and_times.write().unwrap().get_mut(key) {
         *republish_ready = false;
      }
   }

   /// Retrieves all key-entry pairs ready for republishing.
   pub fn get_all_ready_entries(&self) -> Vec<(SubotaiHash, StorageEntry)>  {
      self.entries_and_times.read().unwrap().iter()
         .filter(|&(_, &EntryAndTimes{ republish_ready, ..})| republish_ready)
         .map(|(key, &EntryAndTimes{ ref entry, ..})| (key.clone(), entry.clone()))
         .collect()
   }

   /// the expiration time drops substantially the further away the parent node is from the key, past
   /// a threshold.
   fn calculate_expiration_date(&self, key: &SubotaiHash) -> time::SteadyTime {
      let distance = (&self.parent_id ^ key).height().unwrap_or(0);
      let adjusted_distance  = usize::saturating_sub(distance, self.configuration.expiration_distance_threshold) as u32;
      let clamped_distance = cmp::min(16, adjusted_distance);
      let expiration_factor = 2i64.pow(clamped_distance);
      time::SteadyTime::now() + time::Duration::minutes(60 * self.configuration.base_expiration_time_hrs / expiration_factor)
   }
}

#[cfg(test)]
mod tests {
   use super::*; 
   use {hash, time, node};

   #[test]
   fn expiration_date_calculation_below_distance_threshold() {
      let default_config: node::Configuration = Default::default();
      let id = hash::SubotaiHash::random();
      let storage = Storage::new(id.clone(), default_config.clone());

      // We create a key at distance 1 from our node.
      let key_at_1 = hash::SubotaiHash::random_at_distance(&id, 1);
      let key_at_expf = hash::SubotaiHash::random_at_distance(&id, default_config.expiration_distance_threshold);
      let dummy_entry = StorageEntry::Value(hash::SubotaiHash::random());

      storage.store(key_at_1.clone(), dummy_entry.clone());
      storage.store(key_at_expf.clone(), dummy_entry.clone());
      
      // Both keys should have an expiration date of roughly 24 hours from now.
      let exp_alpha = storage.entries_and_times.read().unwrap().get(&key_at_1).unwrap().expiration.clone();
      let exp_beta  = storage.entries_and_times.read().unwrap().get(&key_at_expf).unwrap().expiration.clone();

      let max_duration = time::Duration::hours(default_config.base_expiration_time_hrs);
      let min_duration = time::Duration::hours(default_config.base_expiration_time_hrs) - time::Duration::minutes(1);

      assert!(exp_alpha <= time::SteadyTime::now() + max_duration);
      assert!(exp_alpha >= time::SteadyTime::now() + min_duration);
      assert!(exp_beta  <= time::SteadyTime::now() + max_duration);
      assert!(exp_beta  >= time::SteadyTime::now() + min_duration);
   }

   #[test]
   fn expiration_date_calculation_over_distance_threshold() {
      let id = hash::SubotaiHash::random();
      let default_config: node::Configuration = Default::default();
      let storage = Storage::new(id.clone(), default_config.clone());

      // We create a key past the distance threshold;
      let excess = 2usize;
      let key = hash::SubotaiHash::random_at_distance(&id, default_config.expiration_distance_threshold + excess);
      let dummy_entry = StorageEntry::Value(hash::SubotaiHash::random());
      storage.store(key.clone(), dummy_entry.clone());
      let expiration = storage.entries_and_times.read().unwrap().get(&key).unwrap().expiration.clone();

      let expiration_factor = 2i64.pow(excess as u32);
      let max_duration = time::Duration::minutes(60 * default_config.base_expiration_time_hrs / expiration_factor );
      let min_duration = time::Duration::minutes(60 * default_config.base_expiration_time_hrs / expiration_factor - 1);
      assert!(expiration <= time::SteadyTime::now() + max_duration);
      assert!(expiration >= time::SteadyTime::now() + min_duration);
   }

   #[test]
   fn setting_and_getting_ready_to_republish_entries() {
      let number_of_keys = 8;
      let storage = Storage::new(hash::SubotaiHash::random(), Default::default());
      let dummy_keys: Vec<_> = (0..number_of_keys).map(|_| hash::SubotaiHash::random()).collect();
      let dummy_entry = StorageEntry::Value(hash::SubotaiHash::random());

      for key in dummy_keys.iter() {
         storage.store(key.clone(), dummy_entry.clone());
      }
   
      assert_eq!(0, storage.get_all_ready_entries().len());

      storage.mark_all_as_ready();
      assert_eq!(number_of_keys, storage.get_all_ready_entries().len());

      storage.mark_as_not_ready(dummy_keys.first().unwrap());
      assert_eq!(number_of_keys - 1, storage.get_all_ready_entries().len());
   }
}
