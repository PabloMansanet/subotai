use time;
use hash::SubotaiHash;
use std::collections::HashMap;
use std::sync::RwLock;
use std::cmp;

pub const MAX_STORAGE: usize = 10000;

/// Distance after which the expiration time for a particular key will begin
/// to drop dramatically. Prevents over-caching.
const BASE_EXPIRATION_TIME_HRS : i64 = 24;
const EXPIRATION_DISTANCE_THRESHOLD : usize = 3;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageEntry {
   Value(SubotaiHash),
   Blob(Vec<u8>),
}

#[derive(Debug, Clone)]
struct EntryAndExpiration {
   entry      : StorageEntry,
   expiration : time::SteadyTime,
}

pub struct Storage {
   entries_and_expirations : RwLock<HashMap<SubotaiHash, EntryAndExpiration> >,
   parent_id               : SubotaiHash,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum StoreResult {
   Success,
   AlreadyPresent,
   StorageFull,
}

impl Storage {
   pub fn new(parent_id: SubotaiHash) -> Storage {
      Storage {
         entries_and_expirations : RwLock::new(HashMap::with_capacity(MAX_STORAGE)),
         parent_id               : parent_id,
      }
   }
   
   pub fn len(&self) -> usize {
      self.entries_and_expirations.read().unwrap().len()
   }

   pub fn is_empty(&self) -> bool {
      self.entries_and_expirations.read().unwrap().is_empty()
   }

   pub fn store(&self, key: SubotaiHash, entry: StorageEntry) -> StoreResult {
      let mut entries_and_expirations = self.entries_and_expirations.write().unwrap();
      let expiration = self.calculate_expiration_date(&key);

      let entry_and_expiration = EntryAndExpiration { entry: entry, expiration: expiration, };
      if entries_and_expirations.len() >= MAX_STORAGE {
         StoreResult::StorageFull
      } else {
         match entries_and_expirations.insert(key, entry_and_expiration) {
            None    => StoreResult::Success,
            Some(_) => StoreResult::AlreadyPresent,
         }
      }
   }

   pub fn get(&self, key: &SubotaiHash) -> Option<StorageEntry> {
      if let Some( &EntryAndExpiration { ref entry, .. } ) = self.entries_and_expirations.read().unwrap().get(key) {
         Some(entry.clone())
      } else {
         None
      }
   }

   /// the expiration time drops substantially the further away the parent node is from the key, past
   /// a threshold.
   fn calculate_expiration_date(&self, key: &SubotaiHash) -> time::SteadyTime {
      let distance = (&self.parent_id ^ &key).height().unwrap_or(0);
      let clamped_distance = cmp::max(1, cmp::min(16, distance));
      let expiration_factor = 2i64.pow(usize::saturating_sub(clamped_distance, EXPIRATION_DISTANCE_THRESHOLD) as u32);
      time::SteadyTime::now() + time::Duration::minutes(60 * BASE_EXPIRATION_TIME_HRS / expiration_factor)
   }
}

#[cfg(test)]
mod tests {
   use super::*; 
   use {storage, hash, time};

   #[test]
   fn expiration_date_calculation_below_distance_threshold() {
      let id = hash::SubotaiHash::random();
      let storage = Storage::new(id.clone());

      // We create a key at distance 1 from our node, and another at distance
      // `EXPIRATION_DISTANCE_FACTOR`
      let key_at_1 = hash::SubotaiHash::random_at_distance(&id, 1);
      let key_at_expf = hash::SubotaiHash::random_at_distance(&id, storage::EXPIRATION_DISTANCE_THRESHOLD);
      let dummy_entry = StorageEntry::Value(hash::SubotaiHash::random());

      storage.store(key_at_1.clone(), dummy_entry.clone());
      storage.store(key_at_expf.clone(), dummy_entry.clone());
      
      // Both keys should have an expiration date of roughly 24 hours from now.
      let exp_alpha = storage.entries_and_expirations.read().unwrap().get(&key_at_1).unwrap().expiration.clone();
      let exp_beta  = storage.entries_and_expirations.read().unwrap().get(&key_at_expf).unwrap().expiration.clone();

      let max_duration = time::Duration::hours(storage::BASE_EXPIRATION_TIME_HRS);
      let min_duration = time::Duration::hours(storage::BASE_EXPIRATION_TIME_HRS) - time::Duration::minutes(1);

      assert!(exp_alpha <= time::SteadyTime::now() + max_duration);
      assert!(exp_alpha >= time::SteadyTime::now() + min_duration);
      assert!(exp_beta  <= time::SteadyTime::now() + max_duration);
      assert!(exp_beta  >= time::SteadyTime::now() + min_duration);
   }

   #[test]
   fn expiration_date_calculation_over_distance_threshold() {
      let id = hash::SubotaiHash::random();
      let storage = Storage::new(id.clone());

      // We create a key past the distance threshold;
      let excess = 2usize;
      let key = hash::SubotaiHash::random_at_distance(&id, storage::EXPIRATION_DISTANCE_THRESHOLD + excess);
      let dummy_entry = StorageEntry::Value(hash::SubotaiHash::random());
      storage.store(key.clone(), dummy_entry.clone());
      let expiration = storage.entries_and_expirations.read().unwrap().get(&key).unwrap().expiration.clone();

      let expiration_factor = 2i64.pow(excess as u32);
      let max_duration = time::Duration::minutes(60 * storage::BASE_EXPIRATION_TIME_HRS / expiration_factor );
      let min_duration = time::Duration::minutes(60 * storage::BASE_EXPIRATION_TIME_HRS / expiration_factor - 1);
      assert!(expiration <= time::SteadyTime::now() + max_duration);
      assert!(expiration >= time::SteadyTime::now() + min_duration);

   }

}



