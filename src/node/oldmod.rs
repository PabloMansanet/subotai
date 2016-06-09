pub mod hash;

#[derive(Debug)]
pub struct Node {
   key : hash::Key,
}

impl Node {
   pub fn new() -> Node {
      Node { key : hash::Key::new() }
   }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
    }
}
