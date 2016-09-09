//! Subotai is a Kademlia based distributed hash table. It's designed to be easy to use, safe
//! and quick. Here are some of the ideas that differentiate it from other DHTs:
//!
//! * **Externally synchronous, internally concurrent**: All public methods are blocking and return
//!  a sane result or an explicit timeout. Internally however, subotai is fully concurrent,
//!  and parallel operations will often help each other complete.
//!
//! * **Introduce nodes first, resolve conflicts later**: Subotai differs to the original Kademlia
//!   implementation in that it gives temporary priority to newer contacts for full buckets. This
//!   makes the network more dynamic and capable to adapt quickly, while still providing protection
//!   against basic `DDoS` attacks in the form of a defensive state.
//!
//! * **Flexible storage**: Any key in the key space can hold any number of different entries with
//!   independent expiration times. 
//!
//! * **Impatient**: Subotai is "impatient", in that it will attempt to never wait for responses from
//! an unresponsive node. Queries are sent in parallel where possible, and processes continue when 
//! a subset of nodes have responded.
//!
//! Subotai supports automatic key republishing, providing a good guarantee that an entry will remain
//! in the network until a configurable expiration date. Manually storing the entry in the network
//! again will refresh the expiration date.
//!
//! Subotai also supports caching to balance intense traffic around a given key.
#![allow(dead_code, unknown_lints, wrong_self_convention)]
#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]

extern crate itertools;
extern crate rand;
extern crate bincode;
extern crate bus;
extern crate time;
extern crate sha1;

pub mod node;
pub mod hash;
mod routing;
mod storage;
mod rpc;

mod error;
pub use error::SubotaiError as SubotaiError;
pub use error::SubotaiResult as SubotaiResult;
