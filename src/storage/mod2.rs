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
   size_bytes    : RwLock<usize>,
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
         size_bytes    : RwLock::new(0),
      }
   }
  
   /// Returns size in bytes.
   pub fn len(&self) -> usize {
      *self.size_bytes.read().unwrap()
   }

   pub fn is_empty(&self) -> bool {
      *self.size_bytes.read().unwrap() == 0
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
      let compound_length = Storage::compound_length(entry);

      if key_groups.contains_key(key) {
         let key_group = key_groups.get_mut(key).unwrap();
         let already_existed = if let Some(preexisting_pair) = key_group.iter_mut().find(|stored_pair| stored_pair.1 == *entry) {
            preexisting_pair.0 = expiration.clone();
            true
         } else {
            false
         };
         if !already_existed {
            if compound_length + self.len() > self.configuration.max_storage {
               return StoreResult::StorageFull;
            }
            key_group.push((expiration.clone(), entry.clone()));
            *self.size_bytes.write().unwrap() += compound_length;
         }
      } else {
         if compound_length + self.len() > self.configuration.max_storage {
            return StoreResult::StorageFull;
         }
         let mut key_group = KeyGroup::new();
         key_group.push((expiration.clone(), entry.clone()));
         key_groups.insert(key.clone(), key_group);
         *self.size_bytes.write().unwrap() += compound_length;
      }
      StoreResult::Success
   }

   fn compound_length(entry: &StorageEntry) -> usize {
      mem::size_of::<time::Tm>() + match entry {
         &StorageEntry::Value(_) => mem::size_of::<SubotaiHash>(),
         &StorageEntry::Blob(ref blob) => blob.len(),
      }
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
