use crypto::sha1::Sha1;
use crypto::digest::Digest;
use itertools::Zip;

pub const KEY_SIZE : usize = 160;
pub const KEY_SIZE_BYTES : usize = KEY_SIZE / 8;

/// Light Wrapper over a Sha1 hash
///
/// We aren't interested in strong cryptography, but rather
/// a simple way to generate 160 bit key identifiers.
#[derive(Debug,Clone,PartialEq)]
pub struct Sha1Hash {
   pub raw : [u8; KEY_SIZE_BYTES],
}

impl Sha1Hash {
   fn blank_hash() -> Sha1Hash {
      Sha1Hash { raw : [0; KEY_SIZE_BYTES] }
   }

   pub fn from_string(s: &str) -> Sha1Hash {
      let mut hash_generator = Sha1::new();
      let mut hash = Sha1Hash::blank_hash();
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

   pub fn xor_distance(hash_alpha : &Self, hash_beta : &Self) -> Sha1Hash {
      let mut distance = Sha1Hash::blank_hash();
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
    fn generating_key_from_string() {
       let input = "The quick brown fox jumps over the lazy dog";
       let output_string = "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12";
       let hash = Sha1Hash::from_string(input);

       assert_eq!(output_string, hash.to_string());
    }

    #[test]
    fn computing_height() {
       let mut test_hash = Sha1Hash::blank_hash();
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
