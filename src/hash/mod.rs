use rand::{thread_rng, Rng};
use itertools::Zip;

pub const KEY_SIZE : usize = 160;
pub const KEY_SIZE_BYTES : usize = KEY_SIZE / 8;

/// Light wrapper over a 160 bit hash
///
/// We aren't interested in strong cryptography, but rather
/// a simple way to generate 160 bit key identifiers.
#[derive(Debug,Clone,PartialEq)]
pub struct Hash160 {
   pub raw : [u8; KEY_SIZE_BYTES],
}

impl Hash160 {
   pub fn blank() -> Hash160 {
      Hash160 { raw : [0; KEY_SIZE_BYTES] }
   }

   pub fn random() -> Hash160 {
      let mut hash = Hash160::blank();
      thread_rng().fill_bytes(&mut hash.raw);
      hash
   }

   pub fn xor_distance(hash_alpha : &Self, hash_beta : &Self) -> Hash160 {
      let mut distance = Hash160::blank();
      for (d, a, b) in Zip::new((&mut distance.raw, &hash_alpha.raw, &hash_beta.raw)) {
         *d = a^b;
      }
      distance
   }

   /// Computes the bit index of the highest "1". Returns None for a blank hash.
   pub fn height(&self) -> Option<usize> {
      let last_nonzero_byte = self.raw.iter().enumerate().rev().find(|&pair| *pair.1 != 0);
      if let Some((index, byte)) = last_nonzero_byte {
         for bit in (0..8).rev() {
            if (byte & (1 << bit)) != 0 {
               return Some((8 * index + bit) as usize)
            }
         }
      }
      None
   }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_generation() {
       assert!(Hash160::random() != Hash160::random());
    }

    #[test]
    fn computing_height() {
       let mut test_hash = Hash160::blank();
       assert!(test_hash.height().is_none());
       
       // First bit
       test_hash.raw[0] = 1;
       assert_eq!(test_hash.height(), Some(0));

       // Fourth bit (index 3)
       test_hash.raw[0] = test_hash.raw[0] | (1 << 3);
       assert_eq!(test_hash.height(), Some(3));

       // Last bit (index 159)
       test_hash.raw[19] = 1 << 7;
       assert_eq!(test_hash.height(), Some(159));
    }
}
