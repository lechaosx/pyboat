# Maintainer guide

This is the repository-level operating guide. Use the
[root setup](README.md#start-here) before running repository checks.

## Verification

Repository-wide checks:

| Surface | Command |
| --- | --- |
| Rust diagnostics | `cargo clippy --workspace --all-targets --all-features` |
| Native Rust workspace | `cargo test --workspace` |
| Rust browser target | `cargo clippy -p browser --target wasm32-unknown-unknown --all-features` |
| Browser and playground | `bun run build` |

Some integration tests open loopback UDP sockets for real WebTransport sessions.
Restricted environments must allow loopback networking for those tests.
Headless capture and its rendering integration tests require a hardware Vulkan
GPU; software adapters are rejected.

A successful build does not validate appearance. The
[procedural playground](README.md#start-here) and browser provide interactive
visual checks.

## Repository map

| Area | Paths |
| --- | --- |
| Shared Rust | [`crates/simulation`](crates/simulation), [`crates/game`](crates/game), [`crates/protocol`](crates/protocol) |
| Rust applications | [`crates/server`](crates/server), [`crates/browser`](crates/browser) |
| Web and tools | [`web`](web), [`playground`](playground), [`tools`](tools) |
