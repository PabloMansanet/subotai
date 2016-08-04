# subotai

Subotai is a Kademlia based DHT (Distributed Hash Table) written in the Rust programming language.

Subotai aims to be simple to use and highly concurrent. The API exposes blocking calls that are easy to reason about, and ergonomic, iterator based mechanisms to listen to events in your nodes.

## concurrency

Subotai exploits the fantastic synchronization primitives in the Rust standard library to achieve high concurrency, while guaranteeing memory safety. Since the hash table isn't fully locked at any point, parallel operations help each other complete with their partial results.
