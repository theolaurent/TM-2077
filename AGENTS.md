# AGENTS.md — TM-2077

Guidance for anyone (agent or human) working here: a real-time audio +
cross-platform (native + WASM) app.

## Be concise

Keep prose and comments tight. Comment the *why*, not the *what* — cut anything
the code already says, and don't restate a rule that lives elsewhere (e.g. the
lint list lives in `Cargo.toml`, not here). This file included.

## No panics in production code

Every panic path is a bug, in all code except `#[cfg(test)]`. Enforced by the
`[lints]` table in `Cargo.toml` (`unsafe_code = forbid`; clippy warns on
`unwrap`/`expect`/`panic`/`unreachable`/`todo`/`indexing_slicing`/…).

Forbidden outside tests: `.unwrap()`, `.expect(...)`, `panic!`, `unreachable!`,
`todo!`, `unimplemented!`, `assert*!`, panicking indexing (`slice[i]`,
`&slice[a..b]`), and panicking arithmetic/conversions.

Do instead:

- **Fallible fns return `anyhow::Result<T>`** — propagate with `?`, add
  `.context("...")`, early-out with `anyhow::bail!`.
- **Options:** `?`, `if let`, `let ... else`, `unwrap_or*`, `.get(...)`.
- **Indexing:** `.get(i)` / `.get(a..b)`, or iterate. Constant in-bounds indexing
  of a fixed-size array (`arr[2]` on a `[T; 4]`) is allowed.
- **Arithmetic:** `checked_*` / `saturating_*` / `clamp`.
- **Locks:** never `.lock().unwrap()`; match and degrade. On a real-time thread a
  poisoned lock means degrade and `return`.
- **`RefCell`:** `try_borrow` / `try_borrow_mut`.
- **Entry points (WASM `main`):** `log::error!` and return.
  `console_error_panic_hook` is a last-resort net, not an error strategy.

## Functional-first discipline

Prefer a functional style; go imperative only when the functional version is
genuinely awkward (see exceptions below).

- **Prefer expressions over statements** — iterator chains, `match`, `if let`,
  `Option`/`Result` combinators over mutating locals in a loop.
- **Favour small pure functions** and test them directly; push side effects to
  the edges.
- **Use [`imbl`] persistent collections** for evolving state: clone (cheap,
  structural-sharing) then update the clone, leaving the original untouched.
- **Immutable by default:** `let`, not `let mut`; avoid `&mut` without payoff.

### Threads & shared state

**Default to message passing; shared mutable state is the exception and must be
justified in a comment.** A channel (`std::sync::mpsc`) transfers ownership —
ruling out deadlocks, poisoning, and races — and keeps real-time threads
lock-free. Prefer a bounded `sync_channel` on the audio path: `try_send`
pre-allocates, never blocks, and drops rather than stalling.

Fall back to shared *mutable* state only for **latest-value-wins** state:

- **Independent fields → struct of atomics behind `Arc`.** Reads can tear across
  fields (usually harmless, self-correcting). Need a coherent snapshot? Pack into
  one atomic, or `ArcSwap<T>` (external crate — only with demonstrated need).
- **Single counter/flag → one atomic behind `Arc`.**
- **Shared in-place mutation → `Arc<Mutex<T>>`** (`RwLock` only if reads
  dominate). Tiny critical section; never `.lock().unwrap()` on a real-time thread.
- **Single-threaded interior mutability → `Rc<RefCell<T>>`.**

Prefer `Rc`/`Arc` to an immutable `T` (shared by cloning the pointer) over
`Box`/`RefCell`/`Cell`; keep the pointee immutable. `Box` is fine when an API
demands it (eframe's `Box<dyn App>`).

### When imperative is right

Don't force these into a functional shape: **audio DSP callbacks** (tight
per-sample loops into preallocated buffers, no allocation), **immediate-mode
painting** (painter calls are side-effecting), **eframe `App` state** (`ui()`
takes `&mut self`). Keep any mutation's scope as small as possible.

## Type safety & totality

Make illegal states unrepresentable so functions can be **total**. An
`Option`/`Result` that exists only because the input can be malformed usually
means the input's type is too wide.

- **Model with enums and structs, not strings or loose primitives.** Parse once
  at the boundary, pass the precise type inward.
- **Push fallibility to the edges;** prefer a total `match` (no catch-all) over a
  partial lookup, so the compiler flags new variants.
- **Watch for** a `_ => return None` / `_ => unreachable!()` arm, or an
  `Err`/`None` that "can't happen" — usually a tighter enum's job.

Judgement, not dogma: don't contort a type to eliminate a genuinely fallible
boundary (I/O, real parsing, device init).

## Before you finish

- `cargo fmt` clean, `cargo test` passes.
- `cargo clippy` and `cargo clippy --target wasm32-unknown-unknown` show no new
  panic-lint warnings.
- New logic favours a functional style; any new mutation falls under an exception
  above.
- New APIs favour total over partial functions.

[`imbl`]: https://docs.rs/imbl
