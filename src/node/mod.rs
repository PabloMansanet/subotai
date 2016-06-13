pub mod xor_node;
pub mod hash;

/// Node trait over key and distance types
pub trait Node<K, D> {
   fn distance(&Self, &Self) -> D;
}
