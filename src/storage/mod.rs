use {time, node};
use hash::SubotaiHash;
use std::collections::HashMap;
use std::sync::RwLock;
use std::cmp;

/// This is the data type that can be stored and retrieved in the Subotai network, 
/// consisting of either another hash or a binary blob.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageEntry {
   Value(SubotaiHash),
   Blob(Vec<u8>),
}

/// Storage entry wrapper that includes management information.
#[derive(Debug, Clone)]
struct ExtendedEntry {
   entry           : StorageEntry,
   expiration      : time::Tm,
   republish_ready : bool,
}

/// Groups of extended entries classified by key.
type KeyGroup = Vec<ExtendedEntry>;

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
   MassStoreFailed,
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
      self.clear_expired_entries();
      if let Some(key_group) = self.key_groups.read().unwrap().get(key) {
         Some(key_group.iter().cloned().map(|extended| extended.entry).collect())
      } else {
         None
      }
   }

   /// Stores an entry in a key_group, with an expiration date, if it wasn't present already.
   /// If it was present, it keeps the latest expiration time and marks as not ready for republishing.
   pub fn store(&self, key: &SubotaiHash, entry: &StorageEntry, expiration: &time::Tm) -> StoreResult {
      if self.is_big_blob(entry) {
         return StoreResult::BlobTooBig;
      }

      // Expiration time is clamped to a reasonable value.
      let expiration = cmp::min(*expiration, time::now() + time::Duration::hours(self.configuration.base_expiration_time_hrs));
      let initial_length = self.len();

      let mut key_groups = self.key_groups.write().unwrap();
      if key_groups.contains_key(key) {
         let key_group = key_groups.get_mut(key).unwrap();
         let already_existed = if let Some(preexisting_pair) = key_group.iter_mut().find(|stored_pair| stored_pair.entry == *entry) {
            preexisting_pair.expiration = cmp::max(preexisting_pair.expiration, expiration); // Take the latest expiration time.
            preexisting_pair.republish_ready = false;
            true
         } else {
            false
         };
         if !already_existed {
            if initial_length > self.configuration.max_storage {
               return StoreResult::StorageFull;
            }
            let new_entry = ExtendedEntry {
               entry           : entry.clone(),
               expiration      : expiration.clone(),
               republish_ready : false,
            };
            key_group.push(new_entry);
         }
      } else {
         if initial_length > self.configuration.max_storage {
            return StoreResult::StorageFull;
         }
         let mut key_group = KeyGroup::new();
         let new_entry = ExtendedEntry {
               entry           : entry.clone(),
               expiration      : expiration.clone(),
               republish_ready : false,
         };
         key_group.push(new_entry);
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

   fn clear_expired_entries(&self) {
      let now = time::now();
      let mut key_groups = self.key_groups.write().unwrap();
      for mut key_group in key_groups.values_mut() {
         key_group.retain(|&ExtendedEntry{ expiration, .. }| now < expiration);
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

   /// Marks all entries as ready for republishing.
   pub fn mark_all_as_ready(&self) {
      let mut key_groups = self.key_groups.write().unwrap();
      let extended_entries = key_groups.values_mut().flat_map(|group| group.iter_mut());
      for &mut ExtendedEntry {ref mut republish_ready, ..} in extended_entries {
         *republish_ready = true;
      }
   }

   /// Retrieves all entries stored in this node, that have a shorter distance to a different,
   /// target node. This is used to republish keys when becoming in contact with a new node.
   pub fn get_entries_closer_to(&self, target: &SubotaiHash)-> Vec<(SubotaiHash, Vec<(StorageEntry, time::Tm)>)> {
      self.key_groups
         .read()
         .unwrap()
         .iter()
         .filter(|&(key, _)| (key ^ target) < (key ^ &self.parent_id))
         .map(|(key, keygroup)| (key.clone(), keygroup.iter().cloned().map(|ext| (ext.entry, ext.expiration)).collect::<Vec<_>>()))
         .collect()
   }

   /// Retrieves all keys and associated data ready for republishing
   pub fn get_all_ready_entries(&self) -> Vec<(SubotaiHash, Vec<(StorageEntry, time::Tm)>)>  {
      self.clear_expired_entries();
      
      let key_groups = self.key_groups.read().unwrap();
      let mut all_ready_entries = Vec::<(SubotaiHash, Vec<(StorageEntry, time::Tm)>)>::new();
      for (key, group) in key_groups.iter() {
         let ready_entries_in_group: Vec<(StorageEntry, time::Tm)> = group
         .iter()
         .filter_map(|ext| if ext.republish_ready { Some((ext.entry.clone(), ext.expiration.clone())) } else { None } )
         .collect();

         if !ready_entries_in_group.is_empty() {
            all_ready_entries.push((key.clone(), ready_entries_in_group));
         }
      }
      all_ready_entries
   }
}

#[cfg(test)]
mod tests {
   use super::*; 
   use {time, node};
   use hash::SubotaiHash;

   #[test]
   fn storing_and_retrieving_on_same_key() {
      let storage = default_storage();
      let key = SubotaiHash::random();
      let entry = StorageEntry::Value(SubotaiHash::random());
      let another_entry = StorageEntry::Blob(Vec::<u8>::new());
      let expiration = time::now() + time::Duration::minutes(30);
      match storage.store(&key, &entry, &expiration) {
         StoreResult::Success => (),
         _ => panic!(),
      }
      match storage.store(&key, &another_entry, &expiration) {
         StoreResult::Success => (),
         _ => panic!(),
      }

      let retrieved_entries = storage.retrieve(&key).unwrap();
      assert_eq!(retrieved_entries.len(), 2);
      assert_eq!(entry, retrieved_entries[0]);
      assert_eq!(another_entry, retrieved_entries[1]);
   }

   #[test]
   fn retrieving_all_ready_entries_across_keys() {
      let storage = default_storage();
      let key_alpha = SubotaiHash::random();
      let key_beta = SubotaiHash::random();
      let expiration = time::now() + time::Duration::minutes(30);

      storage.store(&key_alpha, &StorageEntry::Value(SubotaiHash::random()), &expiration);
      storage.store(&key_alpha, &StorageEntry::Value(SubotaiHash::random()), &expiration);
      storage.store(&key_beta, &StorageEntry::Value(SubotaiHash::random()), &expiration);

      // Not ready by default
      assert_eq!(storage.get_all_ready_entries().len(), 0);
      storage.mark_all_as_ready();
      let ready_entries = storage.get_all_ready_entries();
      assert_eq!(ready_entries.len(), 2);
      assert_eq!(storage.len(), 3);
   }

   #[test]
   fn retrieving_all_entries_closest_to_a_given_id() {
      let storage = default_storage();

      // Key at distance 10 from us.
      let key = SubotaiHash::random_at_distance(&storage.parent_id, 10);
      // Key at distance 3 from us.
      let close_key = SubotaiHash::random_at_distance(&storage.parent_id, 3);
      // Node that is at a distance 5 of the first key, therefore closer to us.
      let other_node_id = SubotaiHash::random_at_distance(&key, 5);

      let expiration = time::now() + time::Duration::minutes(30);
      storage.store(&key, &StorageEntry::Value(SubotaiHash::random()), &expiration);
      storage.store(&close_key, &StorageEntry::Value(SubotaiHash::random()), &expiration);

      let entries = storage.get_entries_closer_to(&other_node_id);

      assert_eq!(entries.len(), 1);
      assert_eq!(entries[0].1.len(), 1);
      assert_eq!(&entries[0].0, &key);
   }

   #[test]
   fn storing_preexisting_entry_updates_to_max_expiration() {
      let now = time::now();
      let storage = default_storage();
      let key = SubotaiHash::random();
      let entry = StorageEntry::Value(SubotaiHash::random());
      let expiration_soon = now + time::Duration::minutes(30);
      let expiration_later = now + time::Duration::hours(10);

      storage.store(&key, &entry, &expiration_soon);
      storage.store(&key, &entry, &expiration_later);

      // Little trick to get the expiration date through the API
      storage.mark_all_as_ready();
      let entries = storage.get_all_ready_entries();
      assert_eq!(entries.len(), 1);
      assert_eq!(entries[0].1.len(), 1);
      assert_eq!(expiration_later, entries[0].1[0].1);
   }

   #[test]
   fn storing_preexisting_entry_keeps_max_expiration() {
      let now = time::now();
      let storage = default_storage();
      let key = SubotaiHash::random();
      let entry = StorageEntry::Value(SubotaiHash::random());
      let expiration_soon = now + time::Duration::minutes(30);
      let expiration_later = now + time::Duration::hours(10);

      // Different order!
      storage.store(&key, &entry, &expiration_later);
      storage.store(&key, &entry, &expiration_soon);

      // Little trick to get the expiration date through the API
      storage.mark_all_as_ready();
      let entries = storage.get_all_ready_entries();
      assert_eq!(entries.len(), 1);
      assert_eq!(entries[0].1.len(), 1);
      assert_eq!(expiration_later, entries[0].1[0].1);
   }

   #[test]
   fn clearing_expired_entries_on_retrieval() {
      let now = time::now();
      let storage = default_storage();
      let key_alpha = SubotaiHash::random();
      let entry_alpha = StorageEntry::Value(SubotaiHash::random());
      let expiration_alpha = now + time::Duration::minutes(30);
      let key_beta = SubotaiHash::random();
      let entry_beta = StorageEntry::Value(SubotaiHash::random());
      let expiration_beta = now - time::Duration::minutes(30); // Expired!

      storage.store(&key_alpha, &entry_alpha, &expiration_alpha);
      storage.store(&key_beta, &entry_beta, &expiration_beta);
      assert_eq!(storage.len(), 2);
      assert!(storage.retrieve(&key_beta).is_none());
      assert!(storage.retrieve(&key_alpha).is_some());
      assert_eq!(storage.len(), 1);
   }

   fn default_storage() -> Storage {
      let default_config: node::Configuration = Default::default();
      Storage::new(SubotaiHash::random(), default_config)
   }
}
