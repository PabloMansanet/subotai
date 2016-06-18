use rand::{thread_rng, Rng};
use itertools::Zip;
use std::ops::BitXor;
use std::cmp::{PartialOrd, Ordering};

pub const KEY_SIZE : usize = 160;
pub const KEY_SIZE_BYTES : usize = KEY_SIZE / 8;

/// Light wrapper over a little endian 160 bit hash
///
/// We aren't interested in strong cryptography, but rather
/// a simple way to generate 160 bit key identifiers.
#[derive(Debug,Clone,PartialEq,Eq)]
pub struct Hash160 {
   pub raw : [u8; KEY_SIZE_BYTES],
}

impl PartialOrd for Hash160 {
   fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
      for (a,b) in self.raw.iter().rev().zip(other.raw.iter().rev()) {
         match a.cmp(b) {
            Ordering::Less => return Some(Ordering::Less),
            Ordering::Greater => return Some(Ordering::Greater),
            Ordering::Equal => ()
         }
      }
      None 
   }
}

impl Ord for Hash160 {
   fn cmp(&self, other: &Self) -> Ordering {
      match self.partial_cmp(other) {
         Some(order) => order,
         None => Ordering::Equal
      }
   }
}

impl<'a, 'b> BitXor<&'b Hash160> for &'a Hash160 {
   type Output = Hash160;

   fn bitxor (self, rhs: &'b Hash160) -> Hash160 {
      let mut result = Hash160::blank();
      for (d, a, b) in Zip::new((&mut result.raw, &self.raw, &rhs.raw)) {
         *d = a^b;
      }
      result
   }
}

impl BitXor for Hash160 {
   type Output = Hash160;

   fn bitxor (self, rhs : Self) -> Hash160 {
      let mut raw = self.raw;
      for (a, b) in raw.iter_mut().zip(rhs.raw.iter()) {
         *a ^= *b;
      }
      self
   }
}

pub struct Zeroes { 
   hash  : Hash160,
   index : usize,
   rev   : usize
}

pub struct Ones { 
   hash  : Hash160,
   index : usize,
   rev   : usize,
}

impl Iterator for Zeroes {
   type Item = usize;

   fn next(&mut self) -> Option<usize> {
      while self.index < self.rev {
         let value_at_index = self.hash.raw[self.index / 8] & (1 << (self.index % 8));
         self.index += 1;
         if value_at_index == 0 {
            return Some(self.index - 1);
         }
      }
      None
   }
}

impl Iterator for Ones {
   type Item = usize;

   fn next(&mut self) -> Option<usize> {
      while self.index < self.rev {
         let value_at_index = self.hash.raw[self.index / 8] & (1 << (self.index % 8));
         self.index += 1;
         if value_at_index != 0 {
            return Some(self.index - 1);
         }
      }
      None
   }
}

impl DoubleEndedIterator for Zeroes {
   fn next_back(&mut self) -> Option<usize> {
      while self.index < self.rev {
         let value_at_rev = self.hash.raw[(self.rev-1) / 8] & (1 << ((self.rev-1) % 8));
         self.rev -= 1;
         if value_at_rev == 0 {
            return Some(self.rev);
         }
      }
      None
   }
}

impl DoubleEndedIterator for Ones {
   fn next_back(&mut self) -> Option<usize> {
      while self.index < self.rev {
         let value_at_rev = self.hash.raw[(self.rev-1) / 8] & (1 << ((self.rev-1) % 8));
         self.rev -= 1;
         if value_at_rev != 0 {
            return Some(self.rev);
         }
      }
      None
   }
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

   pub fn zeroes(self) -> Zeroes {
      Zeroes {
         hash  : self,
         index : 0,
         rev   : KEY_SIZE
      }
   }

   pub fn ones(self) -> Ones {
      Ones {
         hash  : self,
         index : 0,
         rev   : KEY_SIZE
      }
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

   pub fn flip_bit(&mut self, position : usize) {
      if position >= KEY_SIZE { return; }
      let byte = &mut self.raw[position / 8];
      *byte ^= 1 << (position % 8);
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

    #[test]
    fn bit_flipping() {
       let mut test_hash = Hash160::blank();
       test_hash.flip_bit(9);
       assert_eq!(test_hash.raw[1], 2);
       test_hash.flip_bit(9);
       assert_eq!(test_hash.raw[1], 0);
    }

    #[test]
    fn iterating_over_ones() {
       let mut test_hash = Hash160::blank();
       let bits = vec![5usize,20,40];

       for bit in &bits {
          test_hash.flip_bit(*bit);
       }

       println!("{:?}",test_hash);

       for (actual, expected) in test_hash.ones().zip(bits) {
          println!("Doot");
          assert_eq!(actual, expected);
       }
    }
}
