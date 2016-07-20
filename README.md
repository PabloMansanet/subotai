# subotai

Subotai is a Kademlia based DHT (Distributed Hash Table) written in the Rust programming language.

Subotai aims to be simple to use and highly concurrent. The API exposes blocking calls that are easy to reason about, and ergonomic, iterator based mechanisms to listen to events in your nodes.

```rust
let alpha = Node::new().unwrap();
let beta  = Node::new().unwrap();

// Links alpha to beta's network.
alpha.bootstrap(beta.local_info());

let receptions = 
   beta
   .receptions()
   .during(Duration::seconds(1))
   .rpc(PingResponse);

alpha.ping(beta.id());
assert_eq!(receptions.count(), 1);
```

## concurrency

Subotai exploits the fantastic synchronization primitives in the Rust standard library to achieve high concurrency, while guaranteeing memory safety. Since the hash table isn't fully locked at any point, parallel operations help each other complete with their partial results.
