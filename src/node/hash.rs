use std::ops::BitXor;
use crypto::sha1::Sha1;
use crypto::digest::Digest;
use std::fmt;

const KEY_SIZE : usize = 160;
const KEY_SIZE_BYTES : usize = KEY_SIZE / 8;


/// Light Wrapper over a Sha1 hash
#[derive(Debug)]
#[derive(Clone)]
pub struct Sha1Hash {
   pub raw : [u8; KEY_SIZE_BYTES],
}

impl Sha1Hash {
   pub fn new() -> Sha1Hash {
      Sha1Hash { raw : [0; KEY_SIZE_BYTES] }
   }

   pub fn from_string(s: &str) -> Sha1Hash {
      let mut hash_generator = Sha1::new();
      let mut hash = Sha1Hash::new();
      hash_generator.input_str(s);
      hash_generator.result(&mut hash.raw);
      hash
   }

   pub fn to_string(&self) -> String {
      let mut hash_string = String::with_capacity(40);
      for byte in &self.raw {
         hash_string.push_str(&format!("{:x}", byte));
      }
      hash_string
   }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generating_key_from_string() {
       let input = "The quick brown fox jumps over the lazy dog";
       let output_string = "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12";
       let hash = Sha1Hash::from_string(input);

       assert_eq!(output_string, hash.to_string());
    }
}
