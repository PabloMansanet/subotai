# subotai

Subotai is a Kademlia based DHT (Distributed Hash Table) written in the Rust programming language.

Subotai aims to be simple to use and highly concurrent. The API exposes blocking calls that are easy to reason about, and ergonomic, iterator based mechanisms to listen to events in your nodes.

```rust
use subotai::node::Node;
use subotai::hash::SubotaiHash;

let alpha = Node::new().unwrap();
let beta = Node::new().unwrap();

alpha.bootstrap_until(beta.local_info(), 1);

let (key, value) = (SubotaiHash::random(), SubotaiHash::random());
alpha.store(&key, &value);

assert_eq!(beta.retrieve(&key).unwrap(), value);
```

# concurrency

Subotai exploits the fantastic synchronization primitives in the Rust standard library to achieve high concurrency, while guaranteeing memory safety. Since the hash table isn't fully locked at any point, parallel operations help each other complete with their partial results.
