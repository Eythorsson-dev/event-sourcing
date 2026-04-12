---
created: 2026-04-12T09:50:39.770Z
title: Create JS wrapper — export core module as WebAssembly library
area: tooling
files: []
---

## Problem

The event-sourcing library is written in Rust. TypeScript/JavaScript consumers currently have no way to use it. To enable use from browser apps, Node.js, or Deno, the core module needs to be compiled to WebAssembly (wasm32-unknown-unknown or wasm32-wasi target) and wrapped with a TypeScript-friendly JS API.

## Solution

1. Add a `event-sourcing-wasm` crate using `wasm-bindgen` to expose the core public API with `#[wasm_bindgen]` attributes.
2. Use `wasm-pack` to build the package and emit the `.wasm` binary alongside generated JS/TS bindings.
3. Publish as an npm package (or provide build output) so TypeScript consumers can `import` and use the library with full type safety.
4. Scope the initial wrapper to the core projection/log-append API — storage remains host-side (users supply their own storage callbacks or use an in-memory store).
