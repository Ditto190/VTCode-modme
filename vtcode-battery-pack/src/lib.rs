//! # VT Code Battery Pack
//!
//! A curated set of crates for building Rust coding agents, inspired by
//! [Graydon Hoare's battery pack concept](https://smallcultfollowing.com/babysteps/blog/2026/07/15/battery-packs/).
//!
//! A battery pack is a crate whose dependencies are the recommendations.
//! Features designate common sets of crates used together. Categories let
//! consumers pick the pieces they need.
//!
//! ## Quick start
//!
//! ```toml
//! [dependencies]
//! vtcode-battery-pack = { path = "../vtcode-battery-pack", features = ["full"] }
//! ```
//!
//! Or pick a subset:
//!
//! ```toml
//! [dependencies]
//! vtcode-battery-pack = { path = "../vtcode-battery-pack", default-features = false, features = ["core", "llm", "tools"] }
//! ```
//!
//! ## Categories
//!
//! | Feature | Crates | Description |
//! |---------|--------|-------------|
//! | `core` | `vtcode-core`, `vtcode-ui` | Agent runtime, TUI, event loop |
//! | `config` | `vtcode-config` | Config loader |
//! | `commons` | `vtcode-commons` | Shared traits, telemetry, error types |
//! | `macros` | `vtcode-macros` | Procedural macros |
//! | `protocols` | `vtcode-acp`, `vtcode-a2a`, `vtcode-mcp` | A2A, ACP, MCP protocol support |
//! | `llm` | `vtcode-llm` | LLM provider abstraction + 21+ providers |
//! | `skills` | `vtcode-skills` | Skill discovery, loading, validation |
//! | `safety` | `vtcode-safety` | Command safety detection |
//! | `execution` | `vtcode-bash-runner`, `vtcode-exec-events` | Shell sandbox, PTY, telemetry |
//! | `state` | `vtcode-session-store` | Session state, loop persistence, progress |
//! | `tools` | `vtcode-utility-tool-specs`, `vtcode-indexer` | Tool schemas, code indexing, search |
//! | `eval` | `vtcode-eval` | Agent evaluation framework |
//! | `auth` | `vtcode-auth` | OAuth and credential storage |
//!
//! ## Presets
//!
//! - `default` — core runtime + execution + state + tools + auth
//! - `full` — every crate in the pack
//! - `internal` — crates not yet published to crates.io (requires workspace path deps)
//!
//! ## Batteries are more than dependencies
//!
//! The `examples/` directory in this crate contains templates for common
//! project setups. Treat them as starting points, not constraints.

/// Category definitions for the VT Code battery pack.
pub mod categories;

pub use categories::BatteryPackCategory;
