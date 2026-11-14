# ADR 0003: How WebAssembly is tested

Status: accepted (flyingrobots/lumen#23)

## Context

Lumen claims the same image bytes on every platform it supports, and WebAssembly is one of them:
a Geordi runtime or a browser front end would run the library compiled to `wasm32`. Compiling is
not evidence; the golden image has to be rendered on the target and its hash compared with the
same constant the native platforms use. `wasm32` differs from the native targets in two ways that
matter here: `usize` is 32 bits, and a thread cannot be started.

There are two common ways to run Rust tests on WebAssembly:

- `wasm32-unknown-unknown` with `wasm-bindgen-test` and `wasm-pack test --node` (what Bunny's
  WebAssembly job does). It needs `wasm-bindgen-test` as a dev-dependency (with `wasm-bindgen`,
  `js-sys` and their proc-macro crates behind it), `wasm-pack` or a `wasm-bindgen-cli` whose
  version matches the crate, and an extra attribute on every test that runs there.
- `wasm32-wasip1` with the standard test harness, run by a WASI runtime. Cargo builds each test
  binary as a `.wasm` program and runs it through a target runner; the tests are unchanged.

## Decision

- **WebAssembly is tested on `wasm32-wasip1` under Node's built-in WASI** (`node:wasi`), through
  `scripts/wasi-run.mjs` as Cargo's target runner (`CARGO_TARGET_WASM32_WASIP1_RUNNER`). The
  program gets its arguments only: no environment and no preopened directory.
- **No dependency is added.** The Rust standard's dependency rule asks why not the standard
  library or a few lines of local code: the standard test harness and a 20-line script on a
  runtime GitHub's runners already carry do the job, so `wasm-bindgen-test`, `wasm-pack` and a
  separate runtime such as `wasmtime` are not needed.
- **What runs there**: the library's unit tests, `tests/render.rs` (which holds the golden-hash
  test) and `tests/library.rs`, in the test and the release profile. `tests/threads.rs` and
  `tests/cli.rs` do not: one starts threads and the other starts processes, neither of which
  exists on this target.
- **One worker.** `render_pixels` starts `workers - 1` threads, so a one-thread schedule starts
  none and runs on WebAssembly unchanged. A test that runs only on `wasm` asks for two workers and
  expects `RenderError::Spawn`, which shows both that the target cannot start a thread and that
  the passing one-worker renders did not.
- **`wasm32-unknown-unknown` is still compiled** (`cargo check -p lumen --lib`), since that is the
  browser target; its arithmetic is the same `wasm32` instruction set, but it is not run.

## Consequences

- A test that must run on WebAssembly must not start a thread or a process, read a file or the
  environment. A test that has to is marked `#[cfg(not(target_family = "wasm"))]` with a comment
  saying why, or lives in a test file the WebAssembly job does not run.
- Test code, like the library, must not assume a 64-bit `usize`.
- `wasm32-unknown-unknown` with `wasm-bindgen` becomes worth its cost only if lumen gains a
  JavaScript API; then its tests move to `wasm-bindgen-test`, and this record is superseded.
- Node's WASI is marked experimental by Node. If it changes or is withdrawn, the runner is the
  one file to replace (for example with `wasmtime`, which Bunny's performance job uses).
