# Allocator & Memory Behavior

VT Code runs a bursty, sparse workload: `Semaphore`-capped concurrency with
`JoinSet` fans-out of many short-lived Tokio tasks, then workers go idle between
bursts (tool batches, LLM streaming, subagents, periodic health/metrics). This
pattern interacts badly with how some allocators return memory to the OS.

## The problem

For this workload, `mimalloc` (the macOS default) and `glibc` hold RSS **flat
near peak** after a burst instead of returning memory. The mechanism: tasks
allocated on one worker thread are often stolen and dropped on another. In
mimalloc v3, cross-thread frees land on the owning page's `xthread_free` list and
are only reconciled by *future allocation activity*. When Tokio workers park
between bursts, that activity never arrives, so the freed chunks stay stranded
and RSS does not drop. See
<https://pranitha.dev/posts/rust-and-memory-allocators/>.

`jemalloc` avoids this only when its `background_thread` is active: it purges
dirty pages on a decay timer (`dirty_decay_ms`/`muzzy_decay_ms`) independent of
allocation activity. That is why the allocator is selected by target OS.

## Allocator selection

- **Linux (containers / long-lived servers):** `tikv-jemalloc` is the default
  allocator. `background_thread` is supported, so memory returns to the OS
  between bursts. `MALLOC_CONF` (`background_thread:true, dirty_decay_ms:10000,
  muzzy_decay_ms:0`) is applied by the `background_threads` feature.
- **macOS (dev):** `mimalloc` is the default — low-latency, and jemalloc's
  `background_thread` is unsupported on this platform (it prints
  `background_thread currently supports pthread only` and pins like mimalloc),
  so switching would not help.

The selection lives in `src/main.rs` via `cfg(target_os = "linux")`. There is no
runtime flag: to compare the other allocator, build for the other target OS, or
flip the `cfg` in `src/main.rs`.

## Measuring it

Use the built-in benchmark — no provider/API key needed. It reports the active
allocator and a per-burst RSS trajectory:

```bash
cargo run --release --bin vtcode -- bench-allocator
```

It prints `peak_rss_mb`, `post_burst_mb`, `post_idle_mb`, `retained_after_idle_mb`.
The diagnostic signature is `post_idle_mb` staying near `peak_rss_mb` (allocator
is NOT returning memory).

Measured on macOS (dev machine), identical workload (2 bursts × 20 events ×
200 tasks, 2s idle):

| Allocator | Baseline MB | Final MB | Retained | Note |
|---|---|---|---|---|
| mimalloc (macOS default) | 25.2 | 55.0 | +118% | pins |
| jemalloc (Linux default) | 24.8 | 44.9 | +81% | pins on macOS only |

The jemalloc row above was measured on macOS where `background_thread` is
unsupported; on Linux the same build returns memory between bursts (per the
article's container findings).

## Implementation

- Allocator selection is target-gated in `src/main.rs` (`cfg(target_os =
  "linux")` → `tikv_jemallocator`, else `mimalloc`). The jemalloc dependency is
  `[target.'cfg(target_os = "linux")'.dependencies]` so macOS builds avoid the
  C build and keep `mimalloc`.
- RSS sampling lives in `vtcode-commons::memory` (real values on macOS via
  `mach_task_basic_info`, on Linux via `/proc/self/statm` — unlike
  `performance_profiler::get_memory_usage_mb`, which is Linux-only with a fake
  macOS fallback).
- The `bench-allocator` command is implemented in `src/cli/bench_allocator.rs`
  and wired through `vtcode_core::cli::args::BenchAllocatorArgs`.
