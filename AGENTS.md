# AGENTS.md — TM-2077

Guidance for any agent (or human) working in this repository. This is a
real-time audio + cross-platform (native + WASM) app.

## HARD RULE: no panics in production code

No panics in production, full stop. Treat every panic path as a bug. This
applies to all code except `#[cfg(test)]` modules.

**Forbidden in non-test code:**

- `.unwrap()`, `.expect(...)`, `panic!`, `unreachable!`, `todo!`,
  `unimplemented!`, `assert!` / `assert_eq!`.
- Panicking indexing/slicing — `slice[i]`, `&slice[a..b]` — unless the index is
  a compile-time constant provably in bounds. Prefer `.get(i)` / `.get(a..b)`.
- Panicking conversions or arithmetic (e.g. debug-mode overflow). Use
  `checked_*` / `saturating_*` / `clamp`.

Enforced by lints in the `[lints]` table of `Cargo.toml` (not crate-level
`#![...]` attributes), so the whole policy lives in one place:

```toml
[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
all = { level = "warn", priority = -1 }
unwrap_used = "warn"
expect_used = "warn"
panic = "warn"
todo = "warn"
unimplemented = "warn"
unreachable = "warn"
```

**Do instead:**

- **Fallible functions return `anyhow::Result<T>`.** Propagate with `?`, add
  context with `.context("...")`, early-out with `anyhow::bail!` (not a panic).
- **Options:** `?`, `if let`, `let ... else`, `unwrap_or*`, `.get(...)`.
- **Locks:** never `.lock().unwrap()`; match on the result and degrade. On a
  real-time thread, a poisoned lock means degrade gracefully and `return`.
- **`RefCell`:** `try_borrow` / `try_borrow_mut`, not `borrow` / `borrow_mut`.
- **Entry points (WASM `main`):** log with `log::error!` and return.
  `console_error_panic_hook` is a last-resort net, not an error strategy.

**Allowed:** `unwrap_or*`, `.get(...)`, `checked_*`, `saturating_*`, `clamp`
everywhere; `unwrap` / `expect` / `panic!` / `assert!` inside `#[cfg(test)]`
only.

## Functional-first discipline

Prefer a functional style; go imperative only when the functional version is
genuinely awkward (see exceptions below).

- **Prefer expressions over statements.** Compute with iterator chains, `match`,
  `if let`, and `Option`/`Result` combinators instead of mutating locals in a
  loop.
- **Favour pure functions.** Extract logic into small `fn`s with no side
  effects and test them directly. Push side effects to the edges.
- **Use persistent data structures** from [`imbl`] (`Vector`, `HashMap`, etc.)
  for evolving collection state. They are copy-on-write with cheap
  structural-sharing clones: clone the value, then update the clone, so the
  original is left untouched instead of being mutated in place.
- Immutable by default: `let`, not `let mut`; avoid `&mut` without real payoff.

### Threads & shared state

**Default to message passing; shared mutable state is the exception and must be
justified in a comment.** A channel (`std::sync::mpsc`) transfers ownership,
ruling out deadlocks, poisoning, and data races, and keeps real-time threads
lock-free. Prefer a bounded `sync_channel` on the audio path: it pre-allocates,
so `try_send` never allocates or blocks, and drops rather than stalling.

Fall back to shared *mutable* state only for **latest-value-wins** state (you
want the newest value, not the queue of every change). Pick the lightest tool:

- **Independent fields → struct of atomics behind `Arc`.** Writer `store`s,
  reader `load`s. Reads can tear across fields — usually harmless and
  self-correcting. Need a **coherent** snapshot? Pack into one `AtomicU32/U64`,
  or `ArcSwap<T>` (external crate — only with demonstrated need).
- **Single counter/flag → one atomic behind `Arc`.**
- **Shared in-place mutation → `Arc<Mutex<T>>`** (`RwLock` only if reads
  dominate). Tiny critical section; never `.lock().unwrap()` on a real-time
  thread.
- **Single-threaded interior mutability → `Rc<RefCell<T>>`**, e.g. state written
  once by a `'static` `spawn_local` closure that can't borrow `self`.

### Sharing data (not communicating)

Prefer `Rc` (single-thread) / `Arc` (cross-thread) to an immutable `T`, shared
by cloning the pointer, over `Box` / `RefCell` / `Cell`. Keep the pointee
immutable. `Box` is fine when an API demands it (eframe's `Box<dyn App>`).

### When imperative is right

Don't force these into a functional shape:

- **Audio DSP callbacks**: tight per-sample loops into preallocated buffers. No
  allocation on the audio thread.
- **Immediate-mode painting**: egui painter calls are side-effecting. Keep value
  computation functional; let only the painting be imperative.
- **eframe `App` state**: `ui()` takes `&mut self`. Compute new values
  functionally, then assign.

Keep any mutation's scope as small as possible.

[`imbl`]: https://docs.rs/imbl

## Type safety & totality

Make illegal states unrepresentable so functions can be **total**. An
`Option`/`Result` that exists only because the input can be malformed usually
means the input's type is too wide.

- **Model with enums and structs, not strings or loose primitives.** Encode a
  value as an enum/struct rather than a `&str` you probe with `.ends_with(...)`.
  Parse once at the boundary, pass the precise type inward.
- **Push fallibility to the edges** so interior functions are total.
- **Prefer a total `match` over a partial lookup** — exhaustive, no catch-all,
  compiler flags new variants.
- **Let the type carry the invariant.** A function taking a fixed-case enum
  can't receive an out-of-range value and needs no error return.

**Watch for:** a `_ => return None` / `_ => unreachable!()` arm; an `Err`/`None`
that "can't actually happen"; reaching for `anyhow` to paper over a case a
tighter enum would rule out.

Judgement, not dogma: don't contort a type to eliminate a genuinely fallible
boundary (I/O, real parsing, device init). Fewer *spurious* error paths, not
zero error handling.

## Before you finish

- `cargo fmt` clean, `cargo test` passes.
- `cargo clippy` and `cargo clippy --target wasm32-unknown-unknown` show no new
  panic-lint warnings; `git grep` for stray `.unwrap()`/`.expect()`/`panic!` in
  non-test code.
- New logic favours a functional style; any new mutation falls under an
  exception above (DSP, painting, framework state).
- New APIs favour total over partial functions — no `Option`/`Result` that only
  handles an impossible case.
