# AGENTS.md — TM-2077

Guidance for any agent (or human) working in this repository.

## HARD RULE: no panics in production code

**Production code MUST NOT panic.** This is a real-time audio + cross-platform
(native + WASM) app; a panic on the audio thread crashes the stream, and a panic
on WASM aborts the whole module. Treat every panic path as a bug.

### Forbidden in non-test code

Do not introduce any of the following in code that ships (i.e. everything except
`#[cfg(test)]` modules):

- `.unwrap()` and `.expect(...)`
- `panic!`, `unreachable!`, `todo!`, `unimplemented!`, `assert!` / `assert_eq!`
- Indexing or slicing that can panic — `slice[i]`, `&slice[a..b]`, `s[a..b]` —
  unless the index is a compile-time constant provably in bounds. Prefer
  `.get(i)` / `.get(a..b)`.
- Integer/float conversions or arithmetic that can panic (e.g. debug-mode
  overflow). Use `checked_*` / `saturating_*` / `clamp` where relevant.

These are enforced by lints in `src/main.rs`:

```rust
#![warn(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
```

Run `cargo clippy` (native) and `cargo clippy --target wasm32-unknown-unknown`
before finishing. Both must be free of `unwrap_used` / `expect_used` / `panic`
warnings.

### What to do instead

- **Fallible functions return `anyhow::Result<T>`.** Propagate with `?` and add
  context with `.context("...")`. `anyhow` is already a dependency. Use
  `anyhow::bail!` for early-out errors; it is *not* a panic.
- **Options:** use `?`, `if let`, `let ... else { return; }`,
  `unwrap_or` / `unwrap_or_else` / `unwrap_or_default`, or `.get(...)`.
- **Locks (`Mutex`, `RwLock`):** never `.lock().unwrap()`. Match on the result
  and degrade gracefully. **In an audio callback, a poisoned lock means emit
  silence / drop the buffer and `return` — never panic.**
- **`RefCell`:** use `try_borrow` / `try_borrow_mut`, not `borrow` /
  `borrow_mut`.
- **Boot/entry points (WASM `main`):** log the error (`log::error!`) and return;
  do not `expect`/`panic!`. `console_error_panic_hook` stays installed only as a
  last-resort safety net, not as an error-handling strategy.

### Allowed

- `unwrap_or*`, `.get(...)`, `checked_*`, `saturating_*`, `clamp`.
- `unwrap()` / `expect()` / `panic!` / `assert!` **inside `#[cfg(test)]` only.**
  Test code is exempt (and is not compiled by a plain `cargo clippy`).

## Before you finish

- `cargo test` passes.
- `cargo clippy` and `cargo clippy --target wasm32-unknown-unknown` show no new
  warnings from the panic lints above.
- No new `.unwrap()`/`.expect()`/`panic!` in non-test code (`git grep` to check).
