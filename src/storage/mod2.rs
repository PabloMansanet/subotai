use {time, node};
use hash::SubotaiHash;
use std::collections::HashMap;
use std::sync::RwLock;
use std::cmp;
use std::mem;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageEntry {
   Value(SubotaiHash),
   Blob(Vec<u8>),
}

type KeyGroup = Vec<(time::Tm, StorageEntry)>; // expiration/entry pair

pub struct Storage {
   key_groups    : RwLock<HashMap<SubotaiHash, KeyGroup> >,
   parent_id     : SubotaiHash,
   configuration : node::Configuration,
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
         key_groups    : RwLock::new(HashMap::with_capacity(configuration.max_storage)),
         parent_id     : parent_id,
         configuration : configuration,
      }
   }
  
   /// Returns number of entries.
   pub fn len(&self) -> usize {
      self.key_groups.read().unwrap().values().flat_map(|group| group.iter()).count()
   }

   pub fn is_empty(&self) -> bool {
      self.len() == 0
   }

   /// Retrieves all entries in a key_group.
   pub fn retrieve(&self, key: &SubotaiHash) -> Option<Vec<StorageEntry>> {
      if let Some(key_group) = self.key_groups.read().unwrap().get(key) {
         Some(key_group.iter().cloned().map(|pair| pair.1).collect())
      } else {
         None
      }
   }

   /// Stores an entry in a key_group, with an expiration date, if it wasn't present already.
   /// If it was present, it keeps the latest expiration time.
   pub fn store(&self, key: &SubotaiHash, entry: &StorageEntry, expiration: &time::Tm) -> StoreResult {
      if self.is_big_blob(entry) {
         return StoreResult::BlobTooBig;
      }

      // Expiration time is clamped to a reasonable value.
      let expiration = cmp::min(*expiration, time::now() + time::Duration::hours(self.configuration.base_expiration_time_hrs));

      let mut key_groups = self.key_groups.write().unwrap();
      if key_groups.contains_key(key) {
         let key_group = key_groups.get_mut(key).unwrap();
         let already_existed = if let Some(preexisting_pair) = key_group.iter_mut().find(|stored_pair| stored_pair.1 == *entry) {
            preexisting_pair.0 = cmp::max(preexisting_pair.0, expiration); // Take the latest expiration time.
            true
         } else {
            false
         };
         if !already_existed {
            if self.len() > self.configuration.max_storage {
               return StoreResult::StorageFull;
            }
            key_group.push((expiration.clone(), entry.clone()));
         }
      } else {
         if self.len() > self.configuration.max_storage {
            return StoreResult::StorageFull;
         }
         let mut key_group = KeyGroup::new();
         key_group.push((expiration.clone(), entry.clone()));
         key_groups.insert(key.clone(), key_group);
      }
      StoreResult::Success
   }

   fn is_big_blob(&self, entry: &StorageEntry) -> bool {
      match entry {
         &StorageEntry::Blob(ref vec) => vec.len() > self.configuration.max_storage_blob_size,
         _ => false,
      }
   }

   pub fn clear_expired_entries(&self) {
      let now = time::now();
      let mut key_groups = self.key_groups.write().unwrap();
      for mut key_group in key_groups.values_mut() {
         key_group.retain(|&(time, _)| now < time);
      }

      // We clear the keygroups that have run out of entries.
      let empty_keys: Vec<_> = key_groups
         .iter()
         .filter_map(|(key, group)| if group.is_empty() { Some(key) } else { None })
         .cloned()
         .collect();

      for key in empty_keys {
         key_groups.remove(&key);
      }
   }

   ///// Marks all key-entry pairs as ready for republishing.
   //pub fn mark_all_as_ready(&self) {
   //   for (_, &mut ExtendedEntry {ref mut republish_ready, ..})  in self.extended_entries.write().unwrap().iter_mut() {
   //      *republish_ready = true;
   //   }
   //}

   ///// Marks a particular key-entry pair as not ready for republishing.
   //pub fn mark_as_not_ready(&self, key: &SubotaiHash) {
   //   if let Some( &mut ExtendedEntry {ref mut republish_ready, ..}) = self.extended_entries.write().unwrap().get_mut(key) {
   //      *republish_ready = false;
   //   }
   //}

   ///// Retrieves all key-entry pairs ready for republishing, together with their current expiration date
   //pub fn get_all_ready_entries(&self) -> Vec<(SubotaiHash, StorageEntry, time::Tm)>  {
   //   self.extended_entries.read().unwrap().iter()
   //      .filter(|&(_, &ExtendedEntry{ republish_ready, ..})| republish_ready)
   //      .map(|(key, &ExtendedEntry{ ref entry, ref expiration, ..})| (key.clone(), entry.clone(), expiration.clone()))
   //      .collect()
   //}
}

#[cfg(test)]
mod tests {
   use super::*; 
   use {hash, time, node};
}
