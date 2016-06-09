use std::ops::BitXor;

const KEY_SIZE : usize = 160;
const KEY_SIZE_BYTES : usize = KEY_SIZE / 8;

#[derive(Debug)]
pub struct BinHash {
   raw : [u8; KEY_SIZE_BYTES],
}

impl BinHash {
   pub fn new() -> BinHash {
      BinHash { raw : [0;KEY_SIZE_BYTES] }
   }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
    }
}
