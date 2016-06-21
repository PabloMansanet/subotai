#![allow(dead_code)]
#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]

extern crate itertools;
extern crate rand;
extern crate bincode;

pub mod node;
pub mod hash;
pub mod routing;
pub mod rpc;
