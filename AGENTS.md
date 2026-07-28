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

#### Communicating between threads: message passing first

**Default to message passing. Shared mutable memory is the exception and must be
justified.** To move data or events between threads, reach for a channel
(`std::sync::mpsc`) before any `Arc<Mutex<T>>`. A channel transfers *ownership*
instead of sharing it: the producer sends, the consumer owns what it receives.
That alone rules out whole classes of bug — deadlock, lock poisoning, forgotten
critical sections, data races — and it keeps real-time threads lock-free.

`NativeTuner` is the reference example: the audio-input callback sends fixed-size
`[f32; BLOCK]` sample blocks over a bounded `std::sync::mpsc::sync_channel`;
`poll` receives and owns them. The bounded channel pre-allocates its ring, so
`try_send` never allocates or blocks on the audio thread, and it drops a block
rather than stalling if the consumer falls behind.

Only fall back to shared *mutable* state when a channel genuinely cannot express
the pattern — and say why in a comment. The clear-cut case is
**latest-value-wins** state: you want the single most recent value with
intermediate updates coalesced, not the queue of every change a channel would
deliver. Reach for the lightest tool that fits:

- **A latest value → the lightest atomic that fits.** When the fields carry no
  cross-field invariant that must hold together, a **struct of atomics** behind
  an `Arc` is the simplest tool. `Metronome::control` (`Arc<Control>`, where
  `Control` is `{ bpm: AtomicU32, beats: AtomicU32, running: AtomicBool }`): the
  UI `store`s each field, the audio callback `load`s them. Lock-free, std-only,
  no allocation, nothing to poison. The one caveat is that reads across fields
  can *tear* — a callback may briefly see a new `bpm` with an old `beats` — which
  is harmless here and self-corrects on the next buffer. If you instead need a
  **coherent** snapshot of all fields at once, publish the whole value atomically:
  pack it into one `AtomicU32`/`AtomicU64` (by value, if it fits a word), or use
  `ArcSwap<T>` (by reference, any size — a lock-free crate, so only with a
  demonstrated need).
- **A single counter/flag → an atomic** (`AtomicU32`, `AtomicBool`) behind an
  `Arc`, e.g. `Metronome::beat_count` (the audio callback stores the beat number;
  the UI loads it to drive the needle).
- **In-place mutation under a lock → `Arc<Mutex<T>>`** (or `Arc<RwLock<T>>` only
  when reads dominate with many concurrent readers) — when readers and writers
  genuinely share and mutate the *same* value rather than each field standing
  alone. Keep the critical section tiny; on a real-time thread never
  `.lock().unwrap()`, and a poisoned lock means emit silence and `return`. (No
  current code needs this.)
- **Single-threaded interior mutability** (not cross-thread at all):
  `WebTuner::analyser` (`Rc<RefCell<Option<AnalyserNode>>>`) is written once,
  asynchronously, by a `'static` `spawn_local` closure that cannot borrow `self`
  on the single wasm thread. `Rc`/`RefCell`, not `Arc`/`Mutex`.

#### Sharing data (not communicating): smart pointers

Prefer sharing data through `Rc` / `Arc` rather than reaching for owned-unique or
single-owner interior-mutability wrappers. When possible, use `Rc`/`Arc` instead
of `Box`, `RefCell`, or `Cell`.

- **Immutable shared data:** hold `Rc<T>` (single-thread) or `Arc<T>`
  (cross-thread) to an immutable `T` and share by cloning the pointer. This is
  the default; keep the pointee immutable whenever you can. (For a shared
  *mutable* value across threads, see message passing / atomics above.)
- **`Box`** is fine when an external API demands it (e.g. eframe's
  `Box<dyn App>` in `src/main.rs`). Don't box just to heap-allocate a value you
  own uniquely and never share.

### When imperative is the right call (rule of thumb)

Functional style must not compromise real-time correctness or fight the
framework. These are expected to stay imperative and should **not** be forced
into a functional shape:

- **Audio DSP callbacks** (`src/audio/metronome.rs`, `tuner_native.rs`): tight
  per-sample loops writing into a preallocated buffer (the output buffer, or a
  fixed sample block that is then sent to the consumer). No allocation, no
  persistent structures on the audio thread — mutate in place.
- **Immediate-mode painting** (`src/ui/**`): egui painter calls are inherently
  side-effecting. A plain `for` loop that emits shapes is fine; keep the *value*
  computation functional (e.g. `digit_at` in `src/ui/seg.rs`) and let only the
  painting be imperative.
- **eframe `App` state**: `ui()` receives `&mut self`; updating fields each
  frame is the framework's model. Compute the new values functionally, then
  assign.

If you do use mutation, keep its scope as small as possible.

[`rpds`]: https://docs.rs/rpds

## Type safety & totality

Prefer to make illegal states unrepresentable, so functions can be **total** (no
error path to handle) rather than partial. A partial function that returns
`Option`/`Result` only because its input can be malformed is usually a sign the
input should have a tighter type.

### Do

- **Model with enums and structs, not strings or loose primitives.** A note is a
  `Note { name: Name, accidental: Option<Accidental> }`, not a `&str` you inspect
  with `.ends_with('#')`; a name is a `Name` enum, not a `char` you match with a
  `_ => return None` fallback. Parse/validate once at the boundary, then pass the
  precise type inward.
- **Push fallibility to the edges.** Convert unstructured input (frequencies,
  parsed files, user text) into a domain type as early as possible. Interior
  functions then take that type and are total — no defensive `Option` threading.
- **Prefer a total `match` over a partial lookup.** A `match` over an enum is
  exhaustive and needs no catch-all; the compiler flags the missing case when a
  variant is added. Favour that over `slice.get(i)?` / `map.get(k)?` keyed by a
  stringly value.
- **Let the type carry the invariant.** If a value is always one of N things,
  encode the N in the type. `seg::letter` takes a `Name` (7 total cases), so it
  can't be handed a non-letter and needs no error return — contrast the earlier
  `char` version, which did. `src/note.rs` is the reference example of this whole
  discipline.

### Watch for

- A `_ => return None` / `_ => unreachable!()` arm — often the input type is too
  wide. Narrow it and the arm disappears.
- An `Option`/`Result` return whose `None`/`Err` is "can't actually happen here"
  — make it unrepresentable instead of documenting that it won't occur.
- Reaching for `anyhow` to paper over a case a tighter enum would rule out. (The
  no-panic rule still stands — this is about *removing* the error case, not
  hiding a panic.)

Judgement, not dogma: don't contort a type to eliminate a genuinely fallible
boundary (I/O, real parsing, device init). The goal is fewer *spurious* error
paths, not zero error handling.

## Before you finish

- `cargo fmt` has been run (format after edits; the tree must be `cargo fmt`
  clean — CI/reviewers assume default rustfmt formatting).
- `cargo test` passes.
- `cargo clippy` and `cargo clippy --target wasm32-unknown-unknown` show no new
  warnings from the panic lints above.
- No new `.unwrap()`/`.expect()`/`panic!` in non-test code (`git grep` to check).
- New logic favours a functional style; any new mutation or imperative loop
  falls under one of the exceptions above (DSP, painting, framework state).
- New APIs favour total functions over partial ones (see *Type safety &
  totality*); no `Option`/`Result` that only exists to handle an impossible case.
