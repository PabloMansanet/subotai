use hash::SubotaiHash;
use std::collections::HashMap;
use std::sync::RwLock;

pub const MAX_STORAGE: usize = 10000;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageEntry {
   Value(SubotaiHash),
   Blob(Vec<u8>),
}

pub struct Storage {
   entries: RwLock<HashMap<SubotaiHash, StorageEntry> >,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum StoreResult {
   Success,
   AlreadyPresent,
   StorageFull,
}

impl Storage {
   pub fn new() -> Storage {
      Storage {
         entries: RwLock::new(HashMap::with_capacity(MAX_STORAGE)),
      }
   }
   
   pub fn len(&self) -> usize {
      self.entries.read().unwrap().len()
   }

   pub fn is_empty(&self) -> bool {
      self.entries.read().unwrap().is_empty()
   }

   pub fn store(&self, key: SubotaiHash, entry: StorageEntry) -> StoreResult {
      let mut entries = self.entries.write().unwrap();
      if entries.len() >= MAX_STORAGE {
         StoreResult::StorageFull
      } else {
         match entries.insert(key, entry) {
            None    => StoreResult::Success,
            Some(_) => StoreResult::AlreadyPresent,
         }
      }
   }

   pub fn get(&self, key: &SubotaiHash) -> Option<StorageEntry> {
      self.entries.read().unwrap().get(key).cloned()
   }
}
