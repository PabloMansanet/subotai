use hash::SubotaiHash;
use std::collections::HashMap;
use std::sync::RwLock;

pub const MAX_STORAGE: usize = 300;

pub struct Storage {
   values: RwLock<HashMap<SubotaiHash, SubotaiHash> >,
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
         values: RwLock::new(HashMap::with_capacity(MAX_STORAGE)),
      }
   }
   
   pub fn len(&self) -> usize {
      self.values.read().unwrap().len()
   }

   pub fn is_empty(&self) -> bool {
      self.values.read().unwrap().is_empty()
   }

   pub fn store(&self, key: SubotaiHash, value: SubotaiHash) -> StoreResult {
      let mut values = self.values.write().unwrap();
      if values.len() >= MAX_STORAGE {
         StoreResult::StorageFull
      } else {
         match values.insert(key, value) {
            None    => StoreResult::Success,
            Some(_) => StoreResult::AlreadyPresent,
         }
      }
   }

   pub fn get(&self, key: &SubotaiHash) -> Option<SubotaiHash> {
      self.values.read().unwrap().get(key).cloned()
   }
}
