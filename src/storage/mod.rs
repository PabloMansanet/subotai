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
struct ExtendedEntry {
   entry           : StorageEntry,
   expiration      : time::Tm,
   owner           : SubotaiHash,
   republish_ready : bool,
}

pub struct Storage {
   extended_entries  : RwLock<HashMap<SubotaiHash, ExtendedEntry> >,
   parent_id         : SubotaiHash,
   configuration     : node::Configuration,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum StoreResult {
   Success,
   StorageFull,
   BlobTooBig,
   WrongOwner,
}

impl Storage {
   pub fn new(parent_id: SubotaiHash, configuration: node::Configuration) -> Storage {
      Storage {
         extended_entries  : RwLock::new(HashMap::with_capacity(configuration.max_storage)),
         parent_id         : parent_id,
         configuration     : configuration,
      }
   }
   
   pub fn len(&self) -> usize {
      self.extended_entries.read().unwrap().len()
   }

   pub fn is_empty(&self) -> bool {
      self.extended_entries.read().unwrap().is_empty()
   }

   pub fn store(&self, key: SubotaiHash, entry: StorageEntry, expiration: time::Tm, agent: SubotaiHash) -> StoreResult {
      let mut extended_entries = self.extended_entries.write().unwrap();
      let expiration = cmp::min(expiration, time::now() + time::Duration::hours(self.configuration.base_expiration_time_hrs));

      if self.is_big_blob(&entry) {
         return StoreResult::BlobTooBig;
      }

      let new_extended_entry = ExtendedEntry {
         entry           : entry,
         expiration      : expiration,
         owner           : agent,
         republish_ready : false,
      };

      match extended_entries.remove(&key) {
         Some(mut present_entry) => {
            if present_entry.owner == new_extended_entry.owner {
               extended_entries.insert(key, new_extended_entry);
               StoreResult::Success
            } else {
               let different = present_entry.entry != new_extended_entry.entry ||
                               present_entry.expiration != new_extended_entry.expiration;
               present_entry.republish_ready = false;
               extended_entries.insert(key, present_entry);

               if different {
                  StoreResult::WrongOwner
               } else {
                  StoreResult::Success
               }
            }
         }
         None => {
            if extended_entries.len() >= self.configuration.max_storage {
               StoreResult::StorageFull
            } else {
               extended_entries.insert(key, new_extended_entry);
               StoreResult::Success
            }
         }
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
      if let Some( &ExtendedEntry { ref entry, .. } ) = self.extended_entries.read().unwrap().get(key) {
         Some(entry.clone())
      } else {
         None
      }
   }

   pub fn clear_expired_entries(&self) {
      let now = time::now();
      let mut extended_entries = self.extended_entries.write().unwrap();
      let keys_to_remove: Vec<_>= extended_entries
         .iter()
         .filter_map(|(key, &ExtendedEntry{ expiration, .. })| if expiration < now { Some(key) } else { None })
         .cloned()
         .collect();

      for key in keys_to_remove {
         extended_entries.remove(&key);
      }
   }

   /// Marks all key-entry pairs as ready for republishing.
   pub fn mark_all_as_ready(&self) {
      for (_, &mut ExtendedEntry {ref mut republish_ready, ..})  in self.extended_entries.write().unwrap().iter_mut() {
         *republish_ready = true;
      }
   }

   /// Marks a particular key-entry pair as not ready for republishing.
   pub fn mark_as_not_ready(&self, key: &SubotaiHash) {
      if let Some( &mut ExtendedEntry {ref mut republish_ready, ..}) = self.extended_entries.write().unwrap().get_mut(key) {
         *republish_ready = false;
      }
   }

   /// Retrieves all key-entry pairs ready for republishing, together with their current expiration date
   pub fn get_all_ready_entries(&self) -> Vec<(SubotaiHash, StorageEntry, time::Tm)>  {
      self.extended_entries.read().unwrap().iter()
         .filter(|&(_, &ExtendedEntry{ republish_ready, ..})| republish_ready)
         .map(|(key, &ExtendedEntry{ ref entry, ref expiration, ..})| (key.clone(), entry.clone(), expiration.clone()))
         .collect()
   }
}

#[cfg(test)]
mod tests {
   use super::*; 
   use {hash, time, node};

   #[test]
   fn only_the_owner_is_allowed_to_change_an_entry_or_increase_expiration() {
      let default_config: node::Configuration = Default::default();
      let storage = Storage::new(hash::SubotaiHash::random(), default_config.clone());

      let owner = hash::SubotaiHash::random();
      let entry = StorageEntry::Value(hash::SubotaiHash::random());
      let key = hash::SubotaiHash::random();
      let expiration = time::now() + time::Duration::hours(1);

      match storage.store(key.clone(), entry.clone(), expiration.clone(), owner.clone()) {
         StoreResult::Success => (),
         _ => panic!(),
      }

      let wrong_owner = hash::SubotaiHash::random();
      let later_expiration   = expiration + time::Duration::hours(1);
      let new_entry = StorageEntry::Value(hash::SubotaiHash::random());

      // This is fine, because even though we are a different owner, the entry data and expiration are the same.
      match storage.store(key.clone(), entry.clone(), expiration.clone(), wrong_owner.clone()) {
         StoreResult::Success => (),
         _ => panic!(),
      }

      // This won't work, because as the wrong owner we can't increase the expiration date.
      match storage.store(key.clone(), entry.clone(), later_expiration.clone(), wrong_owner.clone()) {
         StoreResult::WrongOwner => (),
         _ => panic!(),
      }

      // This won't work, because as the wrong owner we can't change the data.
      match storage.store(key.clone(), new_entry.clone(), expiration.clone(), wrong_owner.clone()) {
         StoreResult::WrongOwner => (),
         _ => panic!(),
      }

      // This is fine, because as the right owner we can change the data.
      match storage.store(key.clone(), new_entry.clone(), expiration.clone(), owner.clone()) {
         StoreResult::Success => (),
         _ => panic!(),
      }
      assert_eq!(storage.retrieve(&key).unwrap(), new_entry);

      // This is fine, because as the owner we can increase the expiration date.
      match storage.store(key.clone(), new_entry.clone(), later_expiration.clone(), owner.clone()) {
         StoreResult::Success => (),
         _ => panic!(),
      }
   }

   #[test]
   fn setting_and_getting_ready_to_republish_entries() {
      let number_of_keys = 8;
      let storage = Storage::new(hash::SubotaiHash::random(), Default::default());
      let dummy_keys: Vec<_> = (0..number_of_keys).map(|_| hash::SubotaiHash::random()).collect();
      let dummy_entry = StorageEntry::Value(hash::SubotaiHash::random());
      let owner = hash::SubotaiHash::random();

      for key in dummy_keys.iter() {
         storage.store(key.clone(), dummy_entry.clone(), time::now() + time::Duration::hours(24), owner.clone());
      }
   
      assert_eq!(0, storage.get_all_ready_entries().len());

      storage.mark_all_as_ready();
      assert_eq!(number_of_keys, storage.get_all_ready_entries().len());

      storage.mark_as_not_ready(dummy_keys.first().unwrap());
      assert_eq!(number_of_keys - 1, storage.get_all_ready_entries().len());
   }
}
