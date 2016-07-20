//! # Subotai
//!
//! Subotai is a Kademlia based distributed hash table. It's designed to be easy to use, safe
//! and quick. Here are some of the ideas that differentiate it from other DHTs:
//!
//! * **Externally synchronous, internally concurrent**: I believe blocking calls make it easier
//!   to reason about networking code than callbacks. All public methods are blocking and return
//!   a sane result or an explicit timeout. Internally however, subotai is fully concurrent,
//!   and parallel operations will often help each other complete!
//!
//! * **Introduce nodes first, resolve conflicts later**: ...
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
//! # fn main() {
//!
//! let alpha = Node::new();
//! let beta = Node::new();
//!
//! alpha.bootstrap(beta.local_info());
//!
//! let receptions = beta.receptions().during(time::Duration::seconds(1));
//!
//! alpha.ping(beta.local_info().id);
//!  
//! assert_eq!(receptions.count(), 1);
//! # }
//!
//! ```
#![allow(dead_code)]
#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]

extern crate itertools;
extern crate rand;
extern crate bincode;
extern crate bus;
extern crate time;

pub mod node;
pub mod hash;
pub mod routing;
pub mod rpc;

mod error;
pub use error::SubotaiError as SubotaiError;
pub use error::SubotaiResult as SubotaiResult;
