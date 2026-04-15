# Plexus

A Rust IPC library for message exchange over shared memory. Designed for Windows systems where TCP stack overhead and serialization costs must be minimized.

## Core Features
* **Zero-Copy & Zero-Serialization:** Direct memory access via relative pointers (`RelPtr<T>`) and `#[repr(C)]` layouts, eliminating serialization overhead.
* **Lock-Free Signaling:** High-throughput MPMC ring buffer with strict memory ordering (`Acquire/Release`) for sub-microsecond latency.
* **Crash Resilience:** Two-phase publish protocol and deterministic arena recovery to prevent data corruption after process failure.
* **IPC-Safe Containers:** Custom, memory-stable implementations of `PlexusVec`, `PlexusString`, and `PlexusMap` for complex data structures.
* **Production Observability:** Built-in telemetry for latency, throughput, and memory health diagnostics via the `arena_inspect` tool.
* **Windows-Native:** Optimized memory mapping utilizing `CreateFileMappingW` with custom ACL security policies.

## Architecture

The codebase is organized into three distinct layers:

### 1. Foundation Layer (`src/sys`, `src/arena`, `src/ptr`)
* **OS Primitives:** Safe WinAPI wrappers for `CreateFileMappingW` and `MapViewOfFile` with ACL-based security policies.
* **Shared Arena:** Versioned allocator implementing a two-phase publish protocol (prepare/commit) for crash consistency.
* **Relative Addressing:** Type-safe `Handle<T>` and `RelPtr<T>` primitives with alignment-aware offset arithmetic.

### 2. Data Plane (`src/sync`, `src/ipc`, `src/containers`)
* **Lock-Free Signaling:** MPMC ring buffer utilizing slot sequence numbers and `Acquire/Release` memory ordering.
* **Communication APIs:** High-level `SharedChannel` and `Endpoint` abstractions for safe cross-process data exchange.
* **IPC-Safe Containers:** Custom implementations of `PlexusVec`, `PlexusString`, and `PlexusMap` designed for stable shared memory layouts.

### 3. Reliability & Operations (`src/telemetry`, `tools`, `tests`)
* **Memory Reclamation:** Epoch-based reclamation and orphan detection to manage shared memory lifecycle after process termination.
* **Observability:** Built-in telemetry for p99 latency tracking, queue depth, and failed attachment metrics.
* **Diagnostic Tools:** `arena_inspect` for real-time memory health checks and corruption detection.
* **Verification Suite:** Extensive validation using **Loom** for concurrency, **Miri** for memory safety, and fault-injection for crash recovery.

## Usage

**Prerequisites:** Windows host, Rust stable toolchain.

### Integration

Add Plexus to your `Cargo.toml`. For production, use a pinned git revision:

```toml
[dependencies]
plexus = { git = "https://github.com/rozolini/plexus", rev = "<pin-commit>" }
```

## Basic Example

```rust
use plexus::{MapAccess, SharedEndpoint};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
let endpoint = SharedEndpoint::<u64>::open_or_create_mapping(
"Local\\client-ipc",
256 * 1024,
MapAccess::ReadWrite,
1024,
)?;

    // Producer
    endpoint.send_copy(42)?;

    // Consumer
    let value = endpoint.try_recv_copy()?;
    assert_eq!(value, 42);

    Ok(())
}
```

## Benchmark Results

This repository includes a benchmark suite (plexus-bench/) measuring throughput and latency against a standard TCP + JSON loopback implementation.

### Run benchmarks locally:

```bash
cargo run --release --bin plexus-bench -- --mode tcp --iterations 1000000
cargo run --release --bin plexus-bench -- --mode plexus --iterations 1000000
```

### Example Results (1,000,000 iterations):
| Metric | TCP + JSON Baseline | Plexus Shared-Memory Path | Ratio |
| --- | ---: | ---: | ---: |
| Total Time | 67.440210 s | 0.126517 s | ~533.05x (TCP / Plexus) |
| Avg Latency | 67,440 ns/msg | 126 ns/msg | ~535.24x (TCP / Plexus) |
| Throughput | 14,827.95 msg/s | 7,904,063.64 msg/s | ~533.05x (Plexus / TCP) |

## Testing & Verification

Plexus employs rigorous verification paths to ensure memory safety and concurrency correctness.

### Standard tests:

```bash
cargo test
```

### Concurrency model checks (Loom):

```bash
cargo test --features loom-tests --test model_publish_tests
cargo test --features loom-tests --test model_shared_ring_loom
```

### Undefined behavior checks (Miri): (Requires Nightly toolchain):

```bash
cargo +nightly miri test --test property_tests
```

### Boundary and compatibility:

```bash
cargo test --test fuzz_header_boundaries
cargo test --test compatibility_tests
```

## License

This project is licensed under the [MIT License](LICENSE).




