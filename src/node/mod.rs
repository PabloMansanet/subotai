pub mod xor_node;
pub mod hash;

/// Node trait over a distance type
pub trait Node<D> {
   fn distance(&Self, &Self) -> D;
}
