use hash::SubotaiHash;
use std::collections::HashMap;
use std::sync::RwLock;
use error::{SubotaiResult, SubotaiError};

pub const MAX_STORAGE: usize = 300;

pub struct Table {
   entries: RwLock<HashMap<SubotaiHash, SubotaiHash> >,
}

impl Table {
   pub fn new() -> Table {
      Table {
         entries: RwLock::new(HashMap::with_capacity(MAX_STORAGE)),
      }
   }
   
   pub fn len(&self) -> usize {
      self.entries.read().unwrap().len()
   }

   pub fn is_empty(&self) -> bool {
      self.entries.read().unwrap().is_empty()
   }

   pub fn insert(&self, key: SubotaiHash, value: SubotaiHash) -> SubotaiResult<()> {
      let mut entries = self.entries.write().unwrap();
      if entries.len() >= MAX_STORAGE {
         Err(SubotaiError::StorageFull)
      } else {
         entries.insert(key, value);
         Ok(())
      }
   }

   pub fn get(&self, key: &SubotaiHash) -> Option<SubotaiHash> {
      self.entries.read().unwrap().get(key).cloned()
   }
}
