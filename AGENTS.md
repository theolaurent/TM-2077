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

## Functional-first discipline

Prefer a functional style. Reach for imperative code only when the functional
version would be genuinely awkward or cumbersome (see the exceptions below).

### Do

- **Prefer expressions over statements.** Compute values with iterator chains
  (`map` / `filter` / `fold` / `sum` / `collect`), `match`, `if let`, and
  combinator methods (`Option`/`Result`: `map`, `and_then`, `filter`,
  `unwrap_or*`, `.then_some(...)`), rather than mutating locals in a loop.
- **Favour pure functions.** Extract logic into small `fn`s that take inputs and
  return outputs with no side effects, then test them directly (see
  `keep_last` / `tapped_bpm` in `src/app.rs`). Pure functions are the default;
  push side effects to the edges.
- **Use persistent (immutable) data structures** from the [`rpds`] crate
  (`Vector`, `List`, `HashTrieMap`, `RedBlackTreeMap`, …) for evolving
  collection state. Build a new value with structural sharing (`push_back`,
  `insert`, `remove`) instead of mutating in place. `tap_times` in
  `src/app.rs` is the reference example (`rpds::Vector<f64>`).
- Keep data immutable by default: `let`, not `let mut`; avoid `&mut` unless it
  buys real clarity or performance.

### Smart pointers & shared state

Prefer sharing data through `Rc` / `Arc` rather than reaching for owned-unique or
single-owner interior-mutability wrappers. When possible, use `Rc`/`Arc` instead
of `Box`, `RefCell`, or `Cell`.

- **Immutable shared data:** hold `Rc<T>` (single-thread) or `Arc<T>`
  (cross-thread) to an immutable `T` and share by cloning the pointer. This is
  the default; keep the pointee immutable whenever you can.
- **A single shared counter/flag:** use an atomic (`AtomicU32`, `AtomicBool`)
  behind an `Arc`, e.g. `Metronome::current_beat`.
- **Shared *mutable* state** (multiple owners must read and write the same
  value): `Arc<Mutex<T>>` — or `Arc<RwLock<T>>` only when reads dominate and
  there are many concurrent readers — is the right, simple tool. Keep the
  critical section tiny. Prefer the standard library here: **do not pull in a
  lock-free crate (e.g. `arc-swap`) without a demonstrated need.** Current cases:
  - `Metronome::control` (`Arc<Mutex<Control>>`): the UI thread writes the
    tempo/beat/tone settings; the audio callback reads a `Copy` of them.
  - `NativeTuner::ring` (`Arc<Mutex<Vec<f32>>>`): a growing sample buffer the
    audio-input callback appends to every callback.
  - `WebTuner::analyser` (`Rc<RefCell<Option<AnalyserNode>>>`): written once,
    asynchronously, by a `'static` `spawn_local` closure that cannot borrow
    `self`; single-threaded interior mutability is the right tool.
  Note the no-panic rule still applies: never `.lock().unwrap()` — and in an
  audio callback a poisoned lock means emit silence and `return`.
- **`Box`** is fine when an external API demands it (e.g. eframe's
  `Box<dyn App>` in `src/main.rs`). Don't box just to heap-allocate a value you
  own uniquely and never share.

### When imperative is the right call (rule of thumb)

Functional style must not compromise real-time correctness or fight the
framework. These are expected to stay imperative and should **not** be forced
into a functional shape:

- **Audio DSP callbacks** (`src/audio/metronome.rs`, `tuner_native.rs`): tight
  per-sample loops writing into a preallocated output/ring buffer. No
  allocation, no persistent structures on the audio thread — mutate in place.
- **Immediate-mode painting** (`src/ui/**`): egui painter calls are inherently
  side-effecting. A plain `for` loop that emits shapes is fine; keep the *value*
  computation functional (e.g. `digit_at` in `src/ui/seg.rs`) and let only the
  painting be imperative.
- **eframe `App` state**: `ui()` receives `&mut self`; updating fields each
  frame is the framework's model. Compute the new values functionally, then
  assign.

If you do use mutation, keep its scope as small as possible.

[`rpds`]: https://docs.rs/rpds

## Before you finish

- `cargo test` passes.
- `cargo clippy` and `cargo clippy --target wasm32-unknown-unknown` show no new
  warnings from the panic lints above.
- No new `.unwrap()`/`.expect()`/`panic!` in non-test code (`git grep` to check).
- New logic favours a functional style; any new mutation or imperative loop
  falls under one of the exceptions above (DSP, painting, framework state).
