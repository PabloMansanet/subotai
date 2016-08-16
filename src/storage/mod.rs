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
   expiration      : time::Tm,
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

   pub fn store(&self, key: SubotaiHash, entry: StorageEntry, expiration: time::Tm) -> StoreResult {
      let mut entries_and_times = self.entries_and_times.write().unwrap();

      // We will only store entries up to our base expiration time. (i.e. clamping to the max value)
      let mut expiration = cmp::min(expiration, time::now() + time::Duration::hours(self.configuration.base_expiration_time_hrs));

      // If we already had an entry, we keep the longest expiration time.
      if let Some(old) = entries_and_times.remove(&key) {
         expiration = cmp::max(expiration, old.expiration)
      };

      if entries_and_times.len() >= self.configuration.max_storage {
         StoreResult::StorageFull
      } else if self.is_big_blob(&entry){
         StoreResult::BlobTooBig
      } else {
         let entry_and_times = EntryAndTimes { entry: entry, expiration: expiration, republish_ready: false };
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
      let now = time::now();
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

   /// Retrieves all key-entry pairs ready for republishing, together with their current expiration date
   pub fn get_all_ready_entries(&self) -> Vec<(SubotaiHash, StorageEntry, time::Tm)>  {
      self.entries_and_times.read().unwrap().iter()
         .filter(|&(_, &EntryAndTimes{ republish_ready, ..})| republish_ready)
         .map(|(key, &EntryAndTimes{ ref entry, ref expiration, ..})| (key.clone(), entry.clone(), expiration.clone()))
         .collect()
   }
}

#[cfg(test)]
mod tests {
   use super::*; 
   use {hash, time, node};

   #[test]
   fn storing_and_retrieving_preserves_conservative_expiration_dates() {
      let default_config: node::Configuration = Default::default();
      let storage = Storage::new(hash::SubotaiHash::random(), default_config.clone());
      let dummy_key = hash::SubotaiHash::random();
      let dummy_entry = StorageEntry::Value(hash::SubotaiHash::random());

      // We start with an impossible expiration date
      let long_expiration = time::now() + time::Duration::hours(10 * default_config.base_expiration_time_hrs);
      let clamped_expiration = time::now() + time::Duration::hours(default_config.base_expiration_time_hrs);
      storage.store(dummy_key.clone(), dummy_entry.clone(), long_expiration);

      // We check it gets clamped.
      let retrieved_expiration = match storage.entries_and_times.read().unwrap().get(&dummy_key) {
         None => panic!(),
         Some(&super::EntryAndTimes{ref expiration, ..}) => expiration.clone(),
      };
      assert!(retrieved_expiration < clamped_expiration + time::Duration::minutes(1));

      // We try an expiration date smaller than what we have
      let short_expiration = time::now() + time::Duration::minutes(5);
      storage.store(dummy_key.clone(), dummy_entry.clone(), short_expiration);

      // We check it remains long.
      let retrieved_expiration = match storage.entries_and_times.read().unwrap().get(&dummy_key) {
         None => panic!(),
         Some(&super::EntryAndTimes{ref expiration, ..}) => expiration.clone(),
      };
      assert!(retrieved_expiration > clamped_expiration - time::Duration::minutes(1));
   }

   #[test]
   fn setting_and_getting_ready_to_republish_entries() {
      let number_of_keys = 8;
      let storage = Storage::new(hash::SubotaiHash::random(), Default::default());
      let dummy_keys: Vec<_> = (0..number_of_keys).map(|_| hash::SubotaiHash::random()).collect();
      let dummy_entry = StorageEntry::Value(hash::SubotaiHash::random());

      for key in dummy_keys.iter() {
         storage.store(key.clone(), dummy_entry.clone(), time::now() + time::Duration::hours(24));
      }
   
      assert_eq!(0, storage.get_all_ready_entries().len());

      storage.mark_all_as_ready();
      assert_eq!(number_of_keys, storage.get_all_ready_entries().len());

      storage.mark_as_not_ready(dummy_keys.first().unwrap());
      assert_eq!(number_of_keys - 1, storage.get_all_ready_entries().len());
   }
}
