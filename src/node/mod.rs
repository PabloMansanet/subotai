/// Node trait over key and distance types
pub trait Node<K, D> {
   fn distance(&Self, &Self) -> D;
   //fn ping(&self, destination : &K);
}
