# willreturn / `Never` two-category model — handoff

Tracks the move to the `Never`-or-`willreturn` termination model. The **design of record is
`docs/handoffs/willreturn-approach.md`** — the two-category rules, the soundness argument, the phased
implementation plan, and the test list all live there. This file is only "where we are and how we get
there"; do not restate the design here.

## Where we are (the "here")

- **The aliasing-metadata half is built and green, but uncommitted.** Group borrowing emits
  `!alias.scope`/`!noalias` on `CopyPrimIE` loads, `FunctionCallIE` calls, and `MutateIE` stores, and every
  extern declaration carries `nounwind` (`declareExternFunction` in `Backend/src/function/function.cpp`).
  That already unblocks DSE, read-coalescing, and loop read-promotion across opaque calls. This half is
  tracked in `docs/handoffs/borrow-checker-handoff.md` (Noalias / restrict codegen); it is the foundation
  this endeavor builds on, not part of it.
- **Externs carry `nounwind` only — never `willreturn`, never `mustprogress`.** That is exactly the gap the
  design closes. No `Never` type, no return-path check, no `willreturn` emission exist yet: the design is
  written, the implementation is **not started**.
- **Fixtures already in the tree** become the design's regression tests (approach doc §4.1–4.3, §7.3):
  `src/tests/programs/externs/{restrictdse,restrictloopsole,restrictloopblock}`, asserted in
  `end_to_end_tests/tests/noalias.rs`. Run `cargo nextest run --manifest-path Cargo.toml noalias` (and with
  `VALE_TEST_BACKEND=wasi`) to see them; `git status` for what's uncommitted.

## How we get there

1. **Land the current aliasing work first.** It is done and green but uncommitted — the real loose end. This
   endeavor should start from a clean tree, not stack a large feature on an unlanded pile.
2. **Do the §6.1 spike before any implementation.** The approach doc flags "Inliner attribute merging" as
   the one item that can sink the design — a hand-written `.ll` through `opt -O3` (does the inliner drop
   `mustprogress`/`willreturn` when merging a callee that lacks them?). Cheap, decoupled from Valen, and
   gates everything. If it fails, the opaque-`__valen_hang` defense (approach doc §5.5) must be load-bearing.
3. **Then the phases in the approach doc**, in order: `Never` type (§5.1) → return-path check R2 (§5.2) →
   attribute emission (§5.3) → indirect/dispatch (§5.4) → runtime lowering (§5.5) → differential test infra
   (§5.6). Start with the Phase 0 audit (§5.0) to record what we emit today as a baseline.
4. **First optimization-positive win:** the store-sink (approach doc §4.4) once externs get `willreturn` —
   currently the loop's store stays in the loop precisely because we emit `nounwind` without `willreturn`.

## Timing (why not now)

Deliberately deferred: it is a multi-phase cross-cutting feature (typing + codegen + runtime + tests), its
core payoff (aliasing beats Rust) is already proven without it, and §6.1 is unretired. Pick it up when a
perf signal shows the in-loop store costs something on a real workload, or the borrowing feature is broad
enough that a body of code benefits — and always behind the §6.1 spike.

## Lessons learned

- **`noreturn`, `willreturn`, and `mustprogress` are three different things** (approach doc §1.1, Appendix A).
  `willreturn` ⇒ `mustprogress`, not the reverse; `noreturn` and `willreturn` are not complements (the middle
  — "returns on some inputs, not others" — is most functions). Don't conflate them.
- **rustc emits no `mustprogress` at all** (its fix for rust #28728), so Rust is *more* conservative than
  C++/clang on forward progress: it does not delete a pure unused maybe-infinite call; clang does; Valen will
  match clang. Do not conclude "Rust ≈ C++ here" — that is the wrong direction.
- **The load-bearing invariant: never emit `willreturn` on a call whose continuation is `unreachable`**
  (approach doc §3.2). It holds structurally because both key off the same static return type; every other
  rule in the design exists to protect it.
- **Reads never need `willreturn`; only store-motion/DSE-across-a-call does.** A non-returning callee simply
  never reaches a reload, so hoisting a read is safe with `nounwind` alone — which is why the read-promotion
  wins already work today without any of this.
