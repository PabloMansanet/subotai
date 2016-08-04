//! # Subotai
//!
//! Subotai is a Kademlia based distributed hash table. It's designed to be easy to use, safe
//! and quick. Here are some of the ideas that differentiate it from other DHTs:
//!
//! * **Externally synchronous, internally concurrent**: I find blocking calls make it easier 
//!   to reason about networking code than callbacks. All public methods are blocking and return
//!   a sane result or an explicit timeout. Internally however, subotai is fully concurrent,
//!   and parallel operations will often help each other complete!
//!
//! * **Introduce nodes first, resolve conflicts later**: Subotai differs to the original Kademlia
//!   implementation in that it gives temporary priority to newer contacts for full buckets. This
//!   makes the network more dynamic and capable to adapt quickly, while still providing protection
//!   against basic `DDoS` attacks in the form of a defensive state.
//!
//! * ...
//!
//! # Examples
//!
//! Node ping:
//!
//! ```rust
//! # extern crate time;
//! # extern crate subotai;
//! use subotai::node::Node;
//! use subotai::hash::SubotaiHash;
//! # fn main() {
//!
//! let alpha = Node::new().unwrap();
//! let beta = Node::new().unwrap();
//!
//! alpha.bootstrap_until(beta.local_info(), 1);
//!
//! let (key, value) = (SubotaiHash::random(), SubotaiHash::random());
//! alpha.store(&key, &value);
//!
//! assert_eq!(beta.retrieve(&key).unwrap(), value);
//! # }
//!
//! ```
#![allow(dead_code, unknown_lints, wrong_self_convention)]
#![feature(custom_derive, plugin, vec_deque_contains)]
#![plugin(serde_macros)]

extern crate itertools;
extern crate rand;
extern crate bincode;
extern crate bus;
extern crate time;

pub mod node;
pub mod hash;
mod routing;
mod storage;
mod rpc;

mod error;
pub use error::SubotaiError as SubotaiError;
pub use error::SubotaiResult as SubotaiResult;
