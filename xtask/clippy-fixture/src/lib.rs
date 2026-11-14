//! Seeded uses of every item `clippy.toml` bans, for `cargo run -p xtask -- clippy-bans`.
//!
//! Everything here is behind `--cfg lumen_clippy_fixture`, which only that check passes, so the
//! workspace's ordinary builds compile an empty crate (`build.rs` declares the `cfg`). With it,
//! Clippy must report every module preceded by `// seeded: <rule-id>` under that rule, nothing
//! in a module preceded by `// accepted`, nothing outside those modules, and every
//! `clippy.toml` entry at least once. `forms.rs` holds the compiler layer's seeded forms (ADR
//! 0006's, and those of flyingrobots/lumen#115), `entries.rs` one use of every entry,
//! `look_alikes.rs` the deterministic code they resemble.

#![cfg(lumen_clippy_fixture)]
#![deny(clippy::float_arithmetic)]
#![allow(dead_code, unused_imports, unreachable_pub, reason = "seeded forms, never called")]

mod entries;
mod forms;
mod look_alikes;
