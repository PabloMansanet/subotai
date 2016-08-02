use hash::SubotaiHash;
use std::collections::HashMap;
use std::sync::RwLock;
use error::{SubotaiResult, SubotaiError};

pub const MAX_STORAGE: usize = 300;

pub struct Storage {
   values: RwLock<HashMap<SubotaiHash, SubotaiHash> >,
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

   pub fn insert(&self, key: SubotaiHash, value: SubotaiHash) -> SubotaiResult<()> {
      let mut values = self.values.write().unwrap();
      if values.len() >= MAX_STORAGE {
         Err(SubotaiError::StorageFull)
      } else {
         values.insert(key, value);
         Ok(())
      }
   }

   pub fn get(&self, key: &SubotaiHash) -> Option<SubotaiHash> {
      self.values.read().unwrap().get(key).cloned()
   }
}
