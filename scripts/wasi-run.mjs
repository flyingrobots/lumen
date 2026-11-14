// Runs one wasm32-wasip1 program (a cargo test binary) under Node's built-in WASI and exits
// with its status. Cargo calls it as the target runner:
//
//   CARGO_TARGET_WASM32_WASIP1_RUNNER="node $PWD/scripts/wasi-run.mjs"
//
// The program gets its arguments and nothing else: no environment variables and no
// preopened directories, so a test sees no file and no setting of the host.
// A panic aborts (wasm has no unwinding), which Node reports as an uncaught `RuntimeError:
// unreachable` and a non-zero exit status, so a failing test fails the run.

import { readFile } from "node:fs/promises";
import { argv, exit } from "node:process";
import { WASI } from "node:wasi";

const [program, ...args] = argv.slice(2);
if (program === undefined) {
  console.error("usage: node wasi-run.mjs <program.wasm> [args...]");
  exit(2);
}
const wasi = new WASI({ version: "preview1", args: [program, ...args], env: {}, returnOnExit: true });
const module = await WebAssembly.compile(await readFile(program));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
exit(wasi.start(instance));
