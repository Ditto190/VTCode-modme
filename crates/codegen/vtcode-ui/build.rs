//! Detects whether crossterm resolves to the vendored patch or the registry.
//!
//! The workspace replaces crates.io crossterm with the vendored copy in
//! `patches/crossterm` via `[patch.crates-io]`. That copy adds
//! `Event::ColorSchemeReport`, which upstream crossterm 0.29 does not have.
//! Published crates cannot carry path patches, so `cargo package`/`cargo
//! publish` verification builds resolve the registry copy and must compile
//! without the vendored-only event.
//!
//! A path-patched dependency has no `source`/`checksum` entry in Cargo.lock;
//! the registry copy always does. The nearest ancestor `Cargo.lock` is the
//! active one: the workspace lock for in-tree builds, the package's own
//! generated lock for `cargo package` verification builds.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(vendored_crossterm)");
    if let Some(lock) = find_cargo_lock() {
        if let Ok(contents) = std::fs::read_to_string(&lock) {
            if crossterm_is_path_patched(&contents) {
                println!("cargo::rustc-cfg=vendored_crossterm");
            }
        }
    }
}

fn find_cargo_lock() -> Option<PathBuf> {
    let mut dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").ok()?);
    loop {
        let candidate = dir.join("Cargo.lock");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn crossterm_is_path_patched(lock: &str) -> bool {
    let mut in_crossterm_package = false;
    let mut saw_crossterm_package = false;
    for line in lock.lines() {
        if line.starts_with("[[package]]") {
            in_crossterm_package = false;
        } else if let Some(rest) = line.strip_prefix("name = ") {
            in_crossterm_package = rest == "\"crossterm\"";
            saw_crossterm_package |= in_crossterm_package;
        } else if in_crossterm_package && line.starts_with("source = ") {
            return false;
        }
    }
    saw_crossterm_package
}
