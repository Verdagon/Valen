# Valen Two-Category Return Model: `Never` or `willreturn`

**Implementer design document — termination attributes, forward progress, and LLVM codegen**

Status: design decision made; implementation not started.
Audience: compiler implementer with no prior exposure to LLVM's `willreturn`/`mustprogress`/forward-progress machinery.

---

## 0. How to read this document

This is long on purpose. The topic has a fifteen-year history of subtle miscompiles across C, C++, Rust, Zig, and Julia, and the design we've chosen deliberately does something those languages either avoided or got burned by. You need enough background to recognize the failure shapes when you see them in IR.

Suggested reading order:

1. **§1 Background** — what the problem is, in plain terms. Read this even if you know LLVM well; the vocabulary is confusing (`noreturn`, `willreturn`, `mustprogress` are three different things).
2. **§2 The decision** — the exact rules we're adopting, language-level and codegen-level.
3. **§3 Why it's sound** — the argument, so you can tell when a change would break it.
4. **§4 Worked examples** — every case we've discussed, with the behavior we expect under our model. These become tests.
5. **§5 Implementation plan** — phased.
6. **§6 Things to confirm before shipping** — an ordered checklist with a concrete probe for each. **Item 6.1 is the one that can actually sink the design; do it first.**
7. **§7 Test suite** — categorized, with expected outcomes.
8. **§8 Spec text** — what the language reference must say.
9. **§9 References** — every primary source, with what it contributes.
10. **Appendix A** — LLVM internals cheat sheet.

Anywhere you see **CONFIRM**, that is a fact I believe is true but that you must verify against the LLVM version we pin, because LLVM's behavior here has changed several times.

---

## 1. Background

### 1.1 The three-category world

When an optimizer looks at a call `foo()` it wants to know three things about `foo`:

| Question | Enables | LLVM spelling |
|---|---|---|
| Does `foo` read my memory? | Eliminate / sink my stores | `noalias`, `!noalias`, `memory(...)` |
| Does `foo` write my memory? | Eliminate my reloads | same |
| Does `foo` come back? | Move stores *across* the call, out of loops; delete unused pure calls | `willreturn` |

The third question is the subject of this document. LLVM has three answers:

- **`noreturn`** (function attribute): the function never returns to its caller on *any* path. `abort`, `exit`, `panic`, an intentional infinite loop. Code after a call to it is dead; the frontend/LLVM puts an `unreachable` instruction after the call.
- **`willreturn`** (function attribute, added 2019): the function returns on *every* path — or, precisely, per LangRef: *"a call of this function will either exhibit undefined behavior or comes back and continues execution at a point in the existing call stack that includes the current invocation... If an invocation of an annotated function does not return control back to a point in the call stack, the behavior is undefined."* The patch that introduced it (D62801) summarized: *"This attribute guarantees that the function doesn't have any loop, recursion or terminating function like abort, exit."*
- **Neither** (the default): might return, might not. Blocks on I/O, might deadlock, loops on adversarial input, conditionally exits, might panic.

The important thing to notice: `noreturn` and `willreturn` are **not** complements. The negation of "never returns" is "returns on at least one path," which is much weaker than "returns on every path." Almost every real function lives in the middle — it returns on typical inputs and can fail to return (block, abort, loop) on others.

LLVM's default for an opaque call (separate compilation unit, no LTO, no attributes) is "neither." That default is what blocks the optimization we want.

### 1.2 Forward progress: the C++ shortcut, and why it bit everyone

C++11 added a rule ([intro.progress]): the implementation may assume every thread eventually either terminates, does I/O, touches a `volatile`, or performs an atomic/synchronization operation. Any loop that does none of those may be assumed to terminate. In C++, a side-effect-free infinite loop is undefined behavior and may be deleted. The rationale (Hans Boehm, N1528, 2010) was that guaranteeing termination semantics for loops the compiler can't prove terminate "inhibits certain optimizations" on the *terminating* loops that look the same.

C is narrower: C11 §6.8.5p6 only lets the compiler assume termination for loops with a *non-constant* controlling expression; `while(1)` and `for(;;)` are explicitly preserved.

LLVM, for years, hard-coded the C++ rule for *all* frontends. That produced a family of miscompiles in languages that had never adopted it:

- **Rust #28728** (Ralf Jung, 2015), "LLVM loop optimization can make safe programs crash." Reproduced in §4.6. Safe Rust executed a `match` with no arms because LLVM deleted a `loop {}` and fabricated a value of an uninhabited type.
- **Rust #54049** (2018): `fn bar(i) { if i < 1000 { bar(i % 2) } else { 1234567 } }` compiled to `return 1234567` in release. Infinite recursion assumed to terminate.
- **Zig #1658**, **Julia #40009**, John Regehr's "C Compilers Disprove Fermat's Last Theorem" (a Fermat-search loop with unused result compiled to "found a counterexample").

Rust tried inserting `llvm.sideeffect` into every loop (`-Zinsert-sideeffect`, 2019). It cost 3–30% compile time and 3–7% on check builds and was abandoned. The eventual fix (2020, D85393/D86233/D86844, driven by Atmn Patel and Johannes Doerfert for Rust) was to add a **`mustprogress`** function attribute and *invert LLVM's default*: functions without `mustprogress` are no longer assumed to make progress. Clang emits `mustprogress` on every C++11+ function; rustc emits nothing and is therefore safe. Rust 1.52 (May 2021) shipped this; #28728 closed.

Note the relationships (**Appendix A** has the code):

- `willreturn` ⇒ `mustprogress` (a function that always returns obviously makes progress; `Function::mustProgress()` returns true if either attribute is set).
- `mustprogress` does *not* ⇒ `willreturn` (a function that loops forever incrementing an atomic makes progress and never returns).
- Under `mustprogress`, LLVM's `LoopDeletion` will delete a side-effect-free loop with no exit, replacing it with `unreachable`. This is the #28728 mechanism.

Even C++ has walked part of this back: P2809R3 (JF Bastien, C++26, applied as a defect report to earlier standards) makes *trivial* infinite loops (`for(;;);`) well-defined again.

### 1.3 Where Valen enters

Valen's group-borrowing lets us prove, per region of code, which reference is the sole one reaching a group `g`, and emit `!alias.scope {g}` on accesses and `!noalias {g}` on calls. While building a test that this metadata unblocks dead-store elimination across an opaque call (§4.1), we found the DSE needed the callee to be `nounwind` (fixed; Valen is `panic=abort`, so it's simply true). That led to the observation that *sinking a store out of a loop* across the same opaque call additionally needs `willreturn` (§4.4) — and LLVM won't infer that for an opaque callee.

The question became: can Valen just *assert* `willreturn` on every function that isn't `noreturn`? Rust doesn't (it's at parity with C for opaque calls on this axis). The conservative position says the "neither" category is real and you can't. The architect's position says that for a memory-safe language with synchronized shared state, non-termination nobody can observe isn't worth preserving, and the "neither" category can be collapsed.

We spent a long time trying to break the architect's position. This document is the result. The short version: **it holds, under specific conditions, and the conditions are all things we can enforce structurally.** The rest of the document is those conditions.

---

## 2. The decision

### 2.1 Language-level rules

**R1. There is an uninhabited type `Never`.** No value of type `Never` can exist. It is the return type of functions that never return: `exit`, `panic`/`abort`, the explicit hang construct, event loops that only leave via `exit`. `Never` coerces to any type in expression position (so `let x: u32 = if c { 5 } else { panic() }` type-checks).

**R2. A function whose CFG has no path from entry to a return must be declared `-> Never`.** If a function is declared with an inhabited return type but the structural check (§5.3) finds no returning path, that is a **compile error**, not a warning and not a silent reclassification:

```
error: function `f` is declared to return `u32` but no path through its body returns.
       Declare it `-> Never`, or add a path that returns.
```

This catches `fn f() -> u32 { f() }`, mutual recursion with no base case, and a `-> u32` function whose body is an unconditional infinite loop.

**R3. Intentional non-termination is an explicit construct that has type `Never`.** A bare loop with no exit is *not* a special case in the optimizer's eyes; it must be typed `Never` by rule R2 or it doesn't compile. Provide a dedicated `hang()` / `spin_forever()` intrinsic for the embedded "nothing else to do" case (see §5.5 for how it must lower).

**R4. The type checker is the only source of `unreachable` after a call.** Codegen inserts `unreachable` after a call *only* when the call's static return type is `Never`. There is no other path by which an `unreachable` appears after a call in Valen-emitted IR.

**R5. Unsafe code may not construct a value of an uninhabited type.** No `zeroed::<Never>()`, no transmute to `Never`, no reading a `Never` out of a union or raw pointer, no FFI enum declared with zero variants. This is the same rule Rust adopted (RFC 1892; PR #54667 makes `mem::zeroed::<!>()` panic) after real soundness bugs (#47412, #61696, and the 2024 Rust-for-Linux `impl Zeroable for Infallible` bug). If we ever offer `unreachable_unchecked()`, its documentation must say: *"UB if the preceding call was `willreturn`, i.e. any non-`Never` call."*

**R6. Every function not returning `Never` is assumed to return.** This is the whole point. It means: a function that blocks forever on a socket, a mutex, a channel, or a thread join, and a function that aborts on a bounds-check failure, is *still* "assumed to return." The language spec says what that means (§8): the program's observable behavior up to the point of the block/abort is preserved; nothing after it is guaranteed; unobservable work before it may be elided.

**R7. Aliasing guarantees extend to abort/panic handlers and process exit.** If a call is tagged `!noalias {g}`, then *nothing that runs as a consequence of that call* — including a panic hook, an `atexit` destructor, a signal handler — may access group `g` except through an atomic or `volatile`. This is already implied by group borrowing (an exclusive reference means no other live reference reaches `g`), but the panic-hook case is easy to forget, so it's stated explicitly. See §6.7.

### 2.2 Codegen rules (LLVM attribute emission)

For every **Valen-defined function**:

| Property | Emit |
|---|---|
| Return type is `Never` | `noreturn`. Do **not** emit `willreturn` or `mustprogress`. |
| Return type is inhabited | `willreturn`. |
| Always (panic=abort) | `nounwind`. |
| Never emit | `mustprogress` (redundant: implied by `willreturn`, and *wrong* on `Never` functions), `llvm.loop.mustprogress` loop metadata, `nosync`, `speculatable`, `memory(none)`/`readnone`/`readonly` by hand. Let LLVM infer memory and sync attributes. |

For every **call site**:

| Call kind | Emit on the call instruction |
|---|---|
| Direct call to Valen function | Nothing extra; the callee's function attributes carry it. |
| Direct call to `-> Never` function | `unreachable` after the call (R4). |
| Indirect call (function pointer, closure, virtual/trait dispatch) with `Never` signature | Call-site `noreturn` + `unreachable` after. |
| Indirect call with inhabited signature | Call-site `willreturn nounwind`. |
| Any call the borrow checker proves doesn't touch group `g` | `!noalias {g}` as today. |

For **externs (FFI)**:

| Declaration | Emit |
|---|---|
| `extern fn f() -> Never` | `noreturn nounwind`. Document: UB if the C code actually returns. |
| `extern fn f() -> T` (default) | `willreturn nounwind`. |
| `extern fn f() -> T` marked `may_unwind` / `may_longjmp` | `willreturn` only, no `nounwind`. (C code that `longjmp`s out is the one FFI case that violates *nounwind*, not willreturn; see §6.9.) |

For the **runtime**: the panic/abort entry point, the `hang()` lowering, `exit`, and every blocking primitive (lock, channel receive, join, blocking syscall wrappers) must be emitted or declared so that LLVM sees them as having side effects. Concretely: `noreturn nounwind` for the diverging ones; for blocking ones, *no* `memory(none)`/`readonly`/`nosync`/`speculatable`. If they're implemented in Valen, don't fight LLVM's inference here — they do real I/O or atomics, so inference will be conservative. If they're `extern`, don't decorate them beyond `willreturn nounwind`. See §5.5.

### 2.3 What we are explicitly not doing

- **No effect system for blocking/divergence.** Koka has `div` and `blocking` effects; SPARK has `Always_Terminates`. We considered gating `willreturn` on a "may block" effect and rejected it: it doesn't buy soundness (§3.3), and it reintroduces the "neither" category we're collapsing. An effect system may be added later for other reasons (async coloring, diagnostics); it would be orthogonal to this design.
- **No `llvm.sideeffect` sprinkling.** Rust measured it; it's too expensive and it's the wrong fix.
- **No reliance on LLVM inferring `willreturn`.** LLVM's `FunctionAttrs` infers it only for loop-free functions whose every call is `willreturn`, or `mustprogress` functions that only read memory. In a bounds-checked language that's nearly nothing. We stamp it.
- **No C++-style "infinite loops are UB."** The `Never` type is how you write one, and it's honored.

---

## 3. Why it's sound

This section is the argument. If someone proposes a change and you can't see how it preserves each piece below, stop.

### 3.1 The one dangerous thing an optimizer can do with a wrong `willreturn`

If a call is marked `willreturn` but in reality never comes back (blocks, aborts, hangs), the optimizer treats the code after the call as guaranteed to execute. That licenses:

(a) Sinking a store from before the call to after it (or out of a loop containing the call).
(b) Deleting an unused computation whose only "effect" would have been to hang (if also `readnone`).
(c) Hoisting a load from after the call to before it.
(d) Reasoning backwards from UB-on-poison *after* the call to "this value before the call is not poison" (§3.5).
(e) Treating the call's continuation as reached for liveness — which, if the continuation is `unreachable`, means **"this path is impossible; delete the branch that leads here."**

(a), (b), (c) are unobservable if the call really doesn't return: nothing after it runs, so nobody misses the store, the deleted computation, or notices the early load — *provided* the early load can't fault (memory safety) and no other thread/hardware can see the sunk store (atomics/volatile are barriers; the callee can't see it because of `!noalias`).

(e) is the killer. It's what produced Rust #28728: `foo() -> Null` was really `loop {}`; LLVM assumed progress; the `unreachable` after `match n {}` combined with "the call returns" made the path impossible; safe code crashed. And it's what produces the bounds-check deletion shape: `if idx >= len { hang() }` — if `hang()` is marked as returning *and* is followed by `unreachable`, the `idx >= len` branch is "impossible" and the check is deleted.

### 3.2 The invariant that closes (e)

**Never emit `willreturn` on a call whose continuation is `unreachable`.** By R4, the only calls followed by `unreachable` are calls with `Never` return type; by §2.2, those are emitted `noreturn`, never `willreturn`. So (e) cannot occur. Not "is unlikely" — cannot, because the two attributes are keyed off the same static type at the same call site.

This is the load-bearing invariant of the entire design. Every other piece exists to protect it:

- R2 (structural check) ensures no function is *declared* inhabited while having no returning path — so we never stamp `willreturn` on something that's really a `loop {}`.
- R5 (no uninhabited values in unsafe) ensures no `Never` is ever materialized, so no `unreachable` is ever *actually reached* by a live value.
- Indirect calls key off the *signature* type, which is the same type the type checker used to decide whether to insert `unreachable`. A `-> Never` function pointer can't be assigned a returning function in safe code (return types are part of the pointer type; Rust has the same rule: `fn() -> !` does not coerce to `fn() -> T`); doing it via unsafe transmute violates R5's spirit and is UB.

### 3.3 Why being wrong about an inhabited-return call is harmless

Now consider the inhabited-return case where the callee actually blocks forever or aborts: `let n: u32 = recv(); use(n)`. We stamped `willreturn`; it's false. What can go wrong?

The continuation is real code (it must handle a `u32`), not `unreachable`, so (e) doesn't fire. (a)/(b)/(c) are unobservable as argued. That leaves: could the optimizer move an *observable* operation — an I/O call, an atomic, a `volatile` access, another side-effecting call — from after `recv()` to before it, so the program does something visible that it shouldn't have?

No, on grounds independent of `willreturn`. `recv()` itself has side effects (it does I/O / touches atomics). LLVM cannot reorder two side-effecting operations with respect to each other. LLVM's code-motion utilities (`CodeMoverUtils`, LICM's `canSinkOrHoistInst`) refuse to move anything across a call unless the call is *both* `willreturn` and `nosync`; a blocking primitive is never `nosync`. Speculating a call out of a conditional requires the separate `speculatable` attribute, which was created (D20116, D33774) precisely because `willreturn + readnone + nounwind` was found insufficient. And a blocking or aborting function is never `readnone`, so (b) — deleting the call — can't happen either.

So the *only* thing `willreturn` adds on a blocking call is permission to move unobservable work across it. Everything observable is pinned by the call's own side effects.

The same argument covers **abort**. A function that calls `panic()` on a bounds-check failure is not `willreturn` per LangRef (abort is "neither returns nor UB"; Nuno Lopes models it as a write to a "halt bit"). But from the optimizer's viewpoint abort and hang are the same event: control never continues in this thread, and the call has side effects. Every argument above applies unchanged. This matters because in a bounds-checked, panic=abort language, *nearly every function* has an abort path, so we are stamping a false `willreturn` on nearly everything — not just on the handful of blocking functions. The bet is the same bet, just taken more often.

Note what this means for an effect system: gating `willreturn` on "may block" would exclude the blocking functions but *not* the aborting ones, so it wouldn't make the attribute true; it would just make it true on a slightly larger subset. That's why we didn't bother.

### 3.4 What `willreturn` is actually standing in for

The cleanest way to see why the lie is safe: what the optimizer needs, to sink a store past a call, is

> "If this call comes back, it comes back here (`nounwind`, no longjmp); and if it doesn't come back, nobody can observe that the store didn't happen."

The second clause is what `willreturn` is a proxy for. LLVM demands it because in general the callee might loop forever *while publishing the store's address to another thread*, which would then observe the missing store. In Valen that can't happen: the callee is `!noalias {g}` (can't see the location at all), other threads only see memory through atomics/mutexes (barriers), hardware through `volatile` (never optimized). So the thing `willreturn` protects against is independently ruled out. We're using the attribute LLVM happens to check for a fact we've established another way.

### 3.5 The honest residual

Three things are true and should be written down rather than hand-waved:

**(i) By the letter of LangRef, this is UB.** We are emitting an attribute whose definition is stronger than the truth. Our safety argument is about *what LLVM's passes do*, not about the contract. Precedent that this bet is reasonable: C++ has stamped `mustprogress` on every function (including ones that call `abort()` or block in `pthread_mutex_lock`) since 2011, and there is no documented case in fifteen years of a program blocking in a *library call* being miscompiled by forward progress — every documented miscompile is a user-written side-effect-free loop. Precedent that attribute bets can go wrong: Rust's `noalias` on `&mut` was enabled, disabled, and re-enabled over ~six years because LLVM had bugs exploiting it (#31681, #54878, #84958). Those were LLVM bugs against a *true* attribute; ours is a *false* attribute that we argue is unexploitable. Take both precedents seriously.

**(ii) The backward-poison walk is the one consumer that touches code *before* the call.** `isGuaranteedToTransferExecutionToSuccessor(call)` is true for a `willreturn nounwind` call. `programUndefinedIfUndefOrPoison` and friends use it to walk forward from a value's definition, past the call, to an instruction that would be UB if the value were poison, and conclude the value isn't poison — then simplify code *before* the call. Without `willreturn`, the walk stops at the call. This is real. Whether it's *observable* when the call then hangs depends on whether the pre-call simplification guards something observable. We found no concrete exploit, and the branches most likely to be folded are safety checks whose failure arm is `Never`-typed (which itself stops the walk, since a `noreturn` call doesn't transfer to its successor). This is the mechanism to target in differential testing (§7.5).

**(iii) Alive2 won't catch us.** Alive2 validates that a transformation refines its input *given the attributes as stated*. It will happily confirm LLVM correctly exploited our false `willreturn`. The validation we need is differential: same program at `-O0` and `-O3`, compare everything observable up to the hang/abort. (Alive2 is still useful for checking *LLVM* didn't misuse a true attribute; just don't mistake it for validation of our claim.)

### 3.6 Things that are *not* soundness concerns, and why

- **"Wrong answer instead of hang."** `collatz_len(x)` with an unused result gets deleted; a pure loop that would hang on some input instead lets the program continue. This is the C++ behavior, explicitly accepted by the architect. It is a *spec* choice (§8), not a hole. Rust chose the other way; both are coherent.
- **Stack overflow vs hang.** Infinite recursion in a `-> Never` function: tail-call elimination turns it into a loop, so it hangs instead of overflowing. Rust accepts this too.
- **Core dumps / debuggers on optimized builds** show elided stores. Documented as expected.
- **Example 7's hang→crash** (hoisting a faulting load past a hanging call). Requires a load that can fault. Memory safety rules it out in safe code. In `unsafe` blocks with raw pointers, it's the user's responsibility — same as in Rust — and worth a line in the unsafe docs.

---

## 4. Worked examples

Every example here should become a test (§7). For each: the Valen (or pseudo-Rust) source, what LLVM does under our attributes, and whether that's the intended outcome.

Fixtures from the existing test suite: `noopBarrier` is `extern void vtest_noopBarrier(){}` compiled in a separate object with no LTO — an opaque call the optimizer can't see through. `Ship` is `struct { fuel: int }`.

### 4.1 Dead-store elimination across an opaque call (origin; fixture `restrictdse`)

```
extern func noopBarrier();
exported struct Ship { fuel int; }
exported func do_things<g'>(a &Ship in g, b &Ship in g) {
  set b.fuel = 1;      // second reference into g: neither param is whole-function noalias
  set a.fuel = 2;      // redundant
  noopBarrier();       // opaque; tagged !noalias {g}
  set a.fuel = 3;      // live
}
```

Needed: `nounwind` on `noopBarrier` (without it LLVM assumed an exception handler up-stack could observe `fuel == 2`). **Already fixed; two stores in `build.opt.ll`.** `willreturn` was *not* needed for this straight-line overwrite. Keep as a regression test; also add a variant where `noopBarrier` is replaced by a call to a Valen function that may abort, to confirm the store is still eliminated (it should be: `willreturn nounwind !noalias`).

### 4.2 Read promotion across an opaque call, sole reference (fixture `restrictloopsole`)

```
exported func bump(s &Ship) {           // whole-function noalias
  i = 0;
  while i < 1000 {
    set s.fuel = __copy_prim(s.fuel) + 1;
    noopBarrier();
    set i = i + 1;
  }
}
```

Observed: `s.fuel` loaded once before the loop, carried in a `phi`; no reload after the call. Needs only aliasing + `nounwind`. Reads never need `willreturn`: if the callee never returns, the reload simply never executes. Regression test.

### 4.3 Same, block-scoped restrict (fixture `restrictloopblock`)

Two loops over `a` and `b` sharing group `g`; each loop's accesses tagged with scope metadata. Observed: each field loaded once before its loop. This is the headline "Rust can't express this" result and is driven purely by `!alias.scope`/`!noalias`. Regression test.

### 4.4 Store sinking out of the loop — where `willreturn` first bites

In 4.2/4.3 the *load* is hoisted but the *store* `set s.fuel = ...` stays in the loop, executed every iteration. To sink it to after the loop, LLVM must know `noopBarrier` returns on every iteration; otherwise the value must be in memory at each call in case the call never returns and something else (another thread, via the escaped pointer) reads it.

**Under our model:** `noopBarrier` is an extern with inhabited return type → `willreturn nounwind`. Combined with `!noalias {g}`, LICM should sink the store to the loop exit, leaving one store after 1000 iterations.

**Expected `build.opt.ll`:** one `store` to `s.fuel` after the loop, none inside. **This is the first optimization-positive test to write** (§7.3). If it doesn't fire, investigate LICM's `canSinkOrHoistInst` / promotion (`promoteLoopAccessesToScalars`) requirements — it may additionally want the pointer to be `dereferenceable`/`noalias` at the function level or the loop to be in rotated form. CONFIRM.

### 4.5 The C / Rust / Valen comparison

| | Aliasing facts | `willreturn` on opaque callee |
|---|---|---|
| C | none | no (and `while(1)` is legal) |
| Rust | from `&`/`&mut` | no — rustc emits nothing; LLVM can't infer for opaque |
| Valen | from group borrowing | **yes** (this design) |

Valen exceeds Rust on the third column. For a *visible* callee, LLVM infers `willreturn` for all three equally (if loop-free and abort-free), so the difference is only for opaque calls.

### 4.6 Rust #28728 — the fabricated uninhabited value

```rust
enum Null {}
fn foo() -> Null { loop { } }
fn create_null() -> Null {
    let n = foo();
    let mut i = 0;
    while i < 100 { i += 1; }
    return n;
}
fn use_null(n: Null) -> ! { match n { } }
fn main() { use_null(create_null()); }
```

Mechanism: `foo` is inferred `readnone` (touches no memory; the loop isn't considered). Forward progress lets LLVM assume the loop terminates. `readnone` + `willreturn`-ish + unused result → call deleted. `n` "exists." `match n {}` is `unreachable`. UB; release builds crashed.

**Under our model:** `foo() -> Never { loop {} }` is `noreturn`, not `willreturn`; the call is followed by `unreachable` and is *not* deletable (not `willreturn`, and `hang` lowers with side effects per §5.5). Program hangs, as written. **Test:** port this exactly; must hang at `-O3`. Also the inconsistent variant: `fn foo() -> u32 { loop {} }` must be a **compile error** (R2).

### 4.7 Rust #54049 — infinite recursion with a base case

```rust
pub fn bar(i: i32) -> i32 { if i < 1000 { bar(i % 2) } else { 1234567 } }
```

`bar(0)` recurses forever. Release build returned `1234567`.

**Under our model:** `bar` has a returning path → inhabited return type is legal → `willreturn`. LLVM assumes the recursion terminates and returns the base-case value. **This is accepted behavior** (§3.6): wrong answer instead of hang, no safety issue — a real, valid `i32` is produced. Spec must say so (§8). **Test:** compile at `-O3`, assert it returns *some* `i32` without crashing (don't assert the specific value; LLVM may choose differently across versions).

### 4.8 No base case, inhabited return type

```
fn f() -> u32 { f() }
```

**Under our model: compile error (R2).** There is no path to a return except through a call in the same SCC. Without R2, LLVM would tail-call-eliminate to `loop {}`, then (since `willreturn` ⇒ `mustprogress`) `LoopDeletion` would turn it into `unreachable`, and callers' continuations would be deleted — UB from safe code. R2 exists for exactly this. **Test:** must fail to compile with the R2 diagnostic. Also mutual recursion: `fn a() -> u32 { b() } fn b() -> u32 { a() }` — both must error.

### 4.9 No base case, unit return type

```
fn f() -> () { f() }
```

Also a compile error under R2 (no path returns). If the user wants "call forever, don't care," they write `-> Never`. We discussed allowing `-> ()` here because `()` is trivially constructible and assuming return is harmless; we chose not to special-case it, because R2 is simpler as a uniform rule and the diagnostic tells the user what to write. **Test:** compile error.

### 4.10 The explicit hang

```
fn main() -> Never {
  setup();
  hang();          // or an event loop that only exits via exit()
}
```

`hang()` is `-> Never`. `main` has no returning path → must be `-> Never` → `noreturn`, no `mustprogress`, so the loop inside `hang`'s lowering isn't deleted. **Test:** program runs `setup()` (observable output) then hangs at `-O3`; kill on timeout.

### 4.11 Diverge-on-some-input

```
fn f(x: u32) -> u32 { if x == 0 { hang() } else { 1 } }
fn main() { let x = read_u32(); if x == 0 { print("about to hang"); } let r = f(x); print(r); }
```

`f` has a returning path → `willreturn` (a lie on `x == 0`). Inside `f`, `hang()` is `noreturn` + `unreachable`. **Concern:** after inlining `f` into `main`, does LLVM combine the caller's "call was willreturn" with the now-visible `noreturn` path to fold `x == 0`? The call-site attribute goes away with the call when it's inlined; instructions inside the inlined body don't inherit "willreturn" as an assumption; and `hang()` being `noreturn` *stops* the transfer-to-successor walk rather than enabling it. So we expect: "about to hang" printed, then hang. **This is §6.2 — confirm empirically.** Test at `-O3` with `x = 0`: output must include "about to hang" and the process must not exit.

### 4.12 `Never` function inlined into a `willreturn` caller — THE DANGEROUS ONE

```
fn hang() -> Never { loop {} }           // suppose it lowers to a bare side-effect-free loop
fn check(idx: u32, len: u32) -> u32 { if idx >= len { hang() } idx }
```

`check` is `willreturn` ⇒ `mustprogress`. If LLVM inlines `hang` into `check`, the bare `loop {}` is now inside a `mustprogress` function. `LoopDeletion` deletes side-effect-free infinite loops in `mustprogress` functions, replacing them with `unreachable`. Now `if idx >= len { unreachable }` → `idx >= len` assumed false → **bounds check deleted**. This would be a real soundness hole.

Two independent defenses, both required:

1. **LLVM should drop `mustprogress`/`willreturn` from the caller when inlining a callee that lacks them.** I believe `AttributeFuncs::mergeAttributesForInlining` does this (`setAND` on `MustProgress`, `WillReturn`, `NoSync`, `NoFree`). **CONFIRM — §6.1.** If it does, the inlined loop is safe. But note this *also* means every caller that inlines a `Never` function loses its `willreturn`, which may cascade and cost optimization; measure.
2. **Never emit a bare side-effect-free infinite loop in IR at all.** Lower `hang()` to a call to an opaque runtime function `__valen_hang()` (`noreturn nounwind`, implemented in C as `for(;;) pause();` or similar, in a separate object). Then there is no loop for `LoopDeletion` to delete regardless of caller attributes. **Do this unconditionally** (§5.5). Defense 1 is then belt-and-braces.

**Test:** the above at `-O3` with `idx >= len`; must hang, must not read out of bounds. Also a FileCheck test that `__valen_hang` survives as a call in `build.opt.ll`.

### 4.13 Example 6 — `collatz_len` with unused result

```
fn collatz_len(n: u64) -> u64 { steps = 0; while n != 1 { n = if n%2==0 { n/2 } else { 3n+1 }; steps += 1 } steps }
fn main() { x = read(); collatz_len(x); print("done") }
```

`collatz_len` has a returning path → `willreturn`. LLVM infers `memory(none)`. `willreturn + nounwind + memory(none)` + unused result → call deleted. Prints "done" for every input, including a hypothetical non-terminating one. **Accepted behavior.** **Test:** FileCheck that the call is gone at `-O3`. This is the architect's "deleting it is correct, not a bug."

Note: rustc today does *not* delete this (no `mustprogress`); clang does. We match clang.

### 4.14 Example 7 — the faulting hoist

```
unsafe fn f(p: *const i32, hang_at: usize) -> i32 {
    sum = 0;
    for i in 0..1000 { maybe_hang(i); sum += *p.add(i); }   // OOB at i == hang_at
    sum
}
```

With `maybe_hang` `willreturn`, LLVM may hoist/vectorize the raw load ahead of the call, reading `p[hang_at]` before the hang → segfault instead of hang. Requires a faulting load → only possible with raw pointers → `unsafe`. **Not a safe-code concern.** Document in the unsafe guide: "a raw load after a call that might not return may be executed before that call." **Test (unsafe):** port it; the acceptable outcomes are hang *or* segfault; the point of the test is to make sure a *safe* version (bounds-checked slice) always hangs. Write the safe version as the real test.

### 4.15 Function pointer wrapping a `Never` function

```
fn hang() -> Never { ... }
let fp: fn() -> () = || hang();     // closure body is Never; closure type is -> ()
fp();
print("after");
```

The closure is structurally `noreturn` (its only path calls `hang`). Its *type* is `-> ()`, so the indirect call site gets `willreturn`. That's a lie, and it's the harmless kind: the continuation is real code (`print`), not `unreachable`. Program hangs before "after." **Test:** at `-O3`, "after" must not be printed and the process must not exit. Also the `Never`-typed pointer: `let fp: fn() -> Never = hang; fp(); /* unreachable */` — call site gets `noreturn`; test that it hangs and that `unreachable` is emitted.

### 4.16 Blocking primitives

```
let m = mutex.lock();          // may block forever
set a.fuel = 7;                // a exclusive in g; lock() tagged !noalias {g}?
```

Two sub-cases. (i) If `lock()` is tagged `!noalias {g}`: the store may be sunk past it. Sound: nothing reachable from `lock()` can see `a`. (ii) If the lock *protects* `g` (i.e., `a` is obtained *from* the guard), the borrow checker won't tag the lock call `!noalias {g}` and the store isn't movable. Either way, atomics inside `lock()` make it not-`nosync`, so LICM/`CodeMoverUtils` won't move anything across it anyway. **Test:** a threaded program where thread A stores then locks, thread B locks then reads; at `-O3` B must observe the store. (This is really a test of the existing sync model, but it's the scenario people will ask about.)

### 4.17 `experimental.guard` (LLVM PR #69433) — the shape to recognize

LLVM marked its deoptimization guard intrinsic `willreturn`. A guard *does* continue execution — in the interpreter, not at the return site. LLVM folded a `sdiv` UB-check across it; miscompile. Fix: remove `willreturn`. Why this is *not* our case: after abort/hang, control continues nowhere in this thread; after a guard, it continues somewhere with observable state. The design's safety depends on "continues nowhere." Anything we add later that resumes elsewhere (coroutines? green threads with stack switching?) must be re-examined against this.

---

## 5. Implementation plan

### 5.0 Phase 0 — Audit current emission

Before changing anything, dump what Valen emits today for: a normal function, a function that calls `panic`, a function with a loop, an extern, an indirect call, and `main`. Record the attribute sets. Confirm `nounwind` is present everywhere (panic=abort), that nothing emits `mustprogress`, and note whether LLVM is currently *inferring* `willreturn` on any Valen functions (grep `build.opt.ll`). This is your baseline for the "what did we gain" measurement.

### 5.1 Phase 1 — `Never` type

- Add `Never` as an uninhabited type. Coercion: `Never` → any `T` in expression position. Not a subtype of anything for the purposes of function-pointer types (`fn() -> Never` ≠ `fn() -> T`).
- Make `panic`, `exit`, `abort` return `Never`. Add `hang()` returning `Never`.
- Exhaustiveness/uninhabitedness: a `match` on `Never` needs no arms; code after a `Never`-typed expression is unreachable for the purposes of the unreachable-code lint and definite-initialization. **Only** the type checker inserts `unreachable` in codegen, and only after `Never`-typed calls (R4).
- Decide now whether structs/enums containing `Never` fields are themselves uninhabited (Rust: yes for structs, per-variant for enums). Simplest: yes, and the same R5 rules apply to them.
- Reject, in `unsafe`: any construction of an uninhabited value (R5). Enumerate the primitives (`zeroed`, `uninit`, `transmute`, raw reads, union reads, FFI enum decls) and gate each.

### 5.2 Phase 2 — Structural no-return-path check (R2)

**Where:** after type checking and CFG construction, before monomorphization is fine (the check is about control flow, not types) — but see the note on generics below.

**Intraprocedural step.** For each function, build the CFG. A "return node" is a `return` or fall-off-the-end. Mark a call instruction as a *barrier* if its static callee type is `Never` (direct call to a `-> Never` function, or indirect call through a `-> Never` pointer/method). Compute reachability from entry to any return node through the CFG *ignoring loop back-edges' termination* (i.e., a loop with an exit edge is passable; a loop with no exit edge is not) and treating barriers as non-passable. If a return is reachable, the function "may return"; else it is a candidate `noreturn`.

Note this is *not* termination analysis. `while cond {}` with a `return` after it is passable (the `cond`-false edge exists). `while true {}` — decide whether the frontend constant-folds the condition; if it does, there's no exit edge and the function needs `-> Never`; if it doesn't, it's a lie of the accepted kind. Recommend folding literal `true`/`false` conditions so the diagnostic fires.

**Interprocedural step (least fixpoint).** The intraprocedural step treats calls to *inhabited-return* callees as passable. That's wrong within a cycle of functions that only return via each other. Fix with a fixpoint:

```
mayReturn[g] := true   for every extern / indirect target (trusted by signature)
                       unless its signature is -> Never
mayReturn[f] := false  for every Valen-defined function

repeat until no change:
  for each f with mayReturn[f] == false:
    if exists path entry → return in f's CFG such that
       every call on the path is to some g with mayReturn[g] == true
       (Never-signature calls are barriers as above):
      mayReturn[f] := true

any f still false: report R2 error if f's declared return type is inhabited
```

Direction matters: start pessimistic and *prove* returning. If you start optimistic, mutual recursion never contradicts itself. This is a call-graph-SCC computation; do it per SCC in reverse topological order and the "repeat" is only within an SCC. Cost is linear-ish; every optimizing compiler already runs something this shape (LLVM's `FunctionAttrs` `noreturn` inference; Clang/GCC `-Winfinite-recursion`; rustc's `unconditional_recursion` lint, PR #20373).

**Generics.** A generic function's body is checked once, not per instantiation, if you treat all calls to inhabited-typed callees as passable. The one subtlety: a call to a generic `g<T>() -> T` instantiated with `T = Never` is a barrier at *that* call site. If you check pre-monomorphization you'll miss that (harmless: you'd consider a path returning that in fact hangs — the accepted lie). If you check post-mono you'll catch it and may produce a spurious R2 error on a function that's only `noreturn` for one instantiation. Recommend: run the check pre-mono for the diagnostic; separately, at codegen post-mono, treat `Never`-typed call sites as `noreturn` (R4) regardless of what the diagnostic saw.

**Diagnostics.** The error must name the function, say "no path returns," and suggest `-> Never`. For the recursion case, name the cycle: "every path through `a` calls `b`, and every path through `b` calls `a`."

**What this pass must not do:** silently change the function's attributes. It's an error or nothing.

### 5.3 Phase 3 — Attribute emission

Implement §2.2 exactly. Specifically in the codegen attribute builder:

```
if ret_type == Never:
    add NoReturn, NoUnwind
else:
    add WillReturn, NoUnwind
// never: MustProgress, NoSync, Speculatable, Memory(*)
```

Indirect call sites: same rule keyed on the signature's return type, applied as *call-site* attributes on the `call` instruction. Add `unreachable` after `Never`-typed calls (should already exist from Phase 1).

Externs: `willreturn nounwind` by default; `noreturn nounwind` for `-> Never`; drop `nounwind` if marked `may_unwind`.

Remove any existing emission of `mustprogress` or `llvm.loop.mustprogress` if present.

### 5.4 Phase 4 — Indirect calls and dispatch

- Function pointer / closure types carry return types. Enforce no coercion between `-> Never` and `-> T` pointer types (safe code). Closures whose body is `Never` have closure type `-> Never` unless annotated otherwise; allow `|| hang()` to be typed `-> ()` only by explicit annotation, since that's a deliberate lie.
- Trait/virtual methods: the trait signature's return type governs the call site. An impl that's `-> Never` for a trait method declared `-> T` is fine (its body ends in `noreturn`); the call site through the trait is `willreturn`. An impl that's `-> T` for a method declared `-> Never` must be rejected (it would return into `unreachable`).
- Optional later: closed-world devirtualization to mark a `-> T` indirect site `noreturn` when every possible target is `Never`. Pure optimization; no soundness content. Rust hasn't even done the basic `-> !` pointer case (#64219).

### 5.5 Phase 5 — Runtime lowering of diverging and blocking primitives

- `__valen_hang()`: C, separate object, `noreturn`, body `for(;;) { /* pause / wfi */ }`. **Never** lower `hang()` to an inline loop in Valen IR. Declared in IR as `declare void @__valen_hang() noreturn nounwind`. This eliminates the §4.12 hazard at the root and gives embedded users a real place to attach a debugger.
- Panic/abort entry point: `noreturn nounwind`; must not be `memory(none)`. If it's opaque (C), inference won't touch it. If it's Valen, it does I/O (prints) and calls `abort`, so inference is conservative. Check `build.opt.ll` that it isn't `memory(none)`.
- Blocking primitives (`lock`, `recv`, `join`, blocking syscalls): ensure they're either opaque or contain real atomics/syscalls so LLVM infers side effects and not-`nosync`. **Never** add `readonly`/`memory(none)`/`nosync`/`speculatable` by hand.
- `exit()`: `noreturn nounwind`. Note it runs destructors/atexit (R7).

### 5.6 Phase 6 — Test infrastructure

- FileCheck harness over `build.opt.ll` (exists: `cargo nextest` + `awk` on `tmp/vale-test-runs/<test>/out/build.opt.ll`).
- **Differential runner:** compile a program at `-O0` and `-O3`, run both with a timeout, capture stdout/stderr/exit status, compare *up to the point of hang/abort*. For "must hang" tests, assert timeout at both levels and identical output before it. For "must abort" tests, assert identical output before the abort and identical exit status.
- Pin the LLVM version in CI and re-run the full suite on every LLVM bump. This design is a bet on optimizer behavior; LLVM bumps are when it gets re-tested.

---

## 6. Things to confirm before shipping (ordered by risk)

### 6.1 Inliner attribute merging (CAN SINK THE DESIGN — do first)

**Question:** When LLVM inlines a callee that is *not* `mustprogress`/`willreturn` into a caller that *is*, does it remove `mustprogress`/`willreturn` from the caller?

**Why it matters:** §4.12. If the answer is no and a `Never` function's loop lands in a `willreturn` caller, `LoopDeletion` turns the loop into `unreachable` and guards get deleted.

**How to check:** Read `llvm/lib/IR/Attributes.cpp`, function `AttributeFuncs::mergeAttributesForInlining`, in the pinned LLVM. Look for `MustProgress` and `WillReturn` in the `setAND`/`INLINE_COMPAT` section. Then write the IR by hand:

```llvm
define void @hang() noreturn nounwind { br label %l  l: br label %l }
define i32 @check(i32 %i, i32 %n) willreturn nounwind {
  %c = icmp uge i32 %i, %n
  br i1 %c, label %h, label %ok
h:  call void @hang()  unreachable
ok: ret i32 %i
}
```

Run `opt -O3` and inspect: does `@check` still contain a comparison and a call (or an inlined loop)? If the `icmp` is gone, the design needs defense 2 (§5.5, opaque `__valen_hang`) to be *load-bearing*, not belt-and-braces — and you must additionally confirm that no other pass can inline/see through `__valen_hang` (it can't if it's in a separate object with no LTO; with LTO, mark it `noinline` and `optnone` or keep it in a non-LTO object).

**Regardless of the answer, implement §5.5.**

### 6.2 Inlining a `willreturn` call site whose body has a `noreturn` path (§4.11)

**Question:** After inlining `f` (function attr `willreturn`) into `main`, is there any pass that treats the `noreturn` `hang()` inside the inlined region as "impossible" because the enclosing call was `willreturn`?

**How to check:** IR from §4.11 by hand; `opt -O3`; the `icmp eq %x, 0` and the call to `@__valen_hang` must both survive. Also grep `InlineFunction.cpp` for `WillReturn` (I know of `MayContainThrowingOrExitingCallAfterCB`, which uses it for a different purpose — attribute propagation on return values; confirm it can't fold branches).

### 6.3 Backward poison propagation (§3.5.ii)

**Question:** Can `isGuaranteedToTransferExecutionToSuccessor` on a `willreturn` blocking call let LLVM simplify code *before* the call in a way that changes observable pre-call behavior when the call then hangs?

**How to check:** Construct: a value `v` computed before the call; a branch on `v` before the call that prints something; after the call, an operation that is UB if `v` is poison (e.g., `getelementptr inbounds` + load, or a `udiv` by `v`). Compare `-O0` vs `-O3` output when the call hangs. Try both "v definitely not poison" and "v could be poison" (from `freeze`-less arithmetic on uninit). Read `ValueTracking.cpp`: `programUndefinedIfUndefOrPoison`, `isGuaranteedNotToBeUndefOrPoison`, and the `isGuaranteedToTransferExecutionToSuccessor` loop in each. Also `SimplifyCFG`'s use of `isGuaranteedToTransferExecutionToSuccessor` in `FoldBranchToCommonDest` / `markAliveBlocks`. Document what you find, even if it's "no observable effect."

### 6.4 Does the store sink actually fire (§4.4)?

The whole point. Write the fixture, look at `build.opt.ll`. If LICM won't sink the store, check: is the loop rotated? is the pointer `noalias` at the function level or only via scope metadata (LICM's promotion path, `promoteLoopAccessesToScalars`, may require the location to be provably not aliased by *any* access in the loop, which scope metadata should satisfy — CONFIRM)? does the store need `!alias.scope` and the call `!noalias` with matching domains? Use `-debug-only=licm`.

### 6.5 Enumerate `willreturn` consumers in the pinned LLVM

`grep -rn "willReturn\|WillReturn\|isGuaranteedToTransferExecutionToSuccessor\|mayHaveSideEffects" llvm/lib/`. For each hit, classify: forward-only (continuation reached), backward (pre-call reasoning), or deletion. Keep the list in the repo. Re-run on each LLVM bump and diff. Known consumers as of the research: `Local.cpp` (`wouldInstructionBeTriviallyDead`), `MustExecute.cpp`/LICM, `CodeMoverUtils` (requires `willreturn && nosync`), `FunctionAttrs.cpp`, `GVNHoist`, `JumpThreading`, `DivRemPairs`, `InstCombine` (`visitUnreachableInst`), `ValueTracking.cpp` (poison), `LoopDeletion` (via `mustProgress()`).

### 6.6 `speculatable` is never inferred onto our functions

Check `build.opt.ll` for `speculatable`. It shouldn't appear on anything with a loop or an abort path. If the Attributor ever infers it (it can, for provably-total functions), that's fine — those functions really are total.

### 6.7 Panic hooks, `atexit`, signal handlers vs `!noalias` (R7)

If Valen has a user-installable panic hook or destructors that run on `exit`, confirm the borrow checker's exclusivity claim can't be violated by them. Concretely: can a panic hook reach a global that is currently exclusively borrowed in group `g` by the panicking frame? If globals are always their own group and can't be exclusively borrowed into a local group, you're fine. If they can, the `!noalias {g}` on the panicking call is false in a way the hook could observe (it would see the pre-DSE value). Decide and document. Signal handlers reading non-atomic non-volatile memory are UB in C and should be in Valen too — say so.

### 6.8 `nounwind` really is true

Under panic=abort every Valen function is `nounwind`. Confirm the runtime's abort path doesn't unwind (no `_Unwind_RaiseException`, no C++ exceptions crossing into Valen frames). Confirm FFI: a C callee that `longjmp`s past a Valen frame violates `nounwind`; the `may_unwind` escape hatch (§2.2) exists for that. Grep `build.opt.ll` for `invoke` — there should be none.

### 6.9 FFI `-> Never` that actually returns

Document as UB. Consider a debug-mode trap after such calls (`call @f; call @__valen_trap_returned_from_never`) so it's caught in testing. Cheap and catches real bugs (Rust has no such guard; declared-`noreturn`-but-returns has caused real crashes in C projects).

### 6.10 Cross-check against Rust's current state

Rust #64219 (indirect `fn() -> !` not marked `noreturn`) — check whether it's still open; if Rust has since done it, look at how they handled the unsafe-pointer-cast case. Also check whether rustc has started emitting `mustprogress` anywhere (as of the research: no). Not blocking; informative.

### 6.11 Measure

After Phase 3, benchmark: (a) the store-sink fixtures, (b) a real workload, (c) how many functions lose `willreturn` through §6.1's `setAND` if you inline `Never` functions (if you keep any inline-able ones — you shouldn't after §5.5). Report the delta vs Phase 0 baseline.

---

## 7. Test suite

Naming: `term_<category>_<name>`. Each test states its expected outcome at `-O0` and `-O3`.

### 7.1 Type checker (compile-time)

| Test | Expect |
|---|---|
| `fn f() -> u32 { f() }` | error R2 |
| `fn a() -> u32 { b() } fn b() -> u32 { a() }` | error R2, names the cycle |
| `fn f() -> () { f() }` | error R2 |
| `fn f() -> u32 { while true {} }` (literal `true`) | error R2 |
| `fn f() -> u32 { while c {} ; 5 }` | ok |
| `fn f() -> u32 { loop { if c { return 5 } } }` | ok (exit edge via return) |
| `fn f() -> Never { loop {} }` | ok |
| `fn f() -> Never { 5 }` | error: returns a value from a `Never` function |
| `fn f() -> Never { g() }` where `g -> u32` | error: falls off the end / returns |
| `fn f() -> u32 { hang() }` | ok (hang is a barrier; but then no return path!) → **error R2** — verify this is what we want: yes, a `-> u32` function whose only path is `hang()` should be `-> Never` |
| `fn f(x) -> u32 { if x { hang() } 1 }` | ok |
| `let p: fn() -> u32 = hang;` | type error (no coercion) |
| `let p: fn() -> Never = returns_u32;` | type error |
| `impl` of trait method `-> Never` with body `-> u32` | error |
| `match never_value {}` | ok, no arms |
| `unsafe { zeroed::<Never>() }` | error R5 |
| `unsafe { transmute::<u8, Never>(0) }` | error R5 |
| `extern enum E {}` (zero variants) used as FFI return | error R5 |
| generic `fn call<T>(f: fn() -> T) -> T { f() }` | ok; instantiate with `Never`: call site `noreturn` |

### 7.2 Attribute emission (FileCheck on `build.ll`, pre-opt)

| Test | Check |
|---|---|
| plain `-> u32` fn | `willreturn nounwind`, NOT `mustprogress`, NOT `noreturn` |
| `-> Never` fn | `noreturn nounwind`, NOT `willreturn`, NOT `mustprogress` |
| fn containing a bounds check | still `willreturn` (we stamp despite the abort path) |
| fn containing a loop | still `willreturn`; loop has NO `llvm.loop.mustprogress` |
| `extern fn f() -> u32` | `declare ... willreturn nounwind` |
| `extern fn f() -> Never` | `declare ... noreturn nounwind` |
| `extern fn f() -> u32` with `may_unwind` | `willreturn`, no `nounwind` |
| indirect call `-> u32` | call-site `willreturn nounwind` |
| indirect call `-> Never` | call-site `noreturn`, followed by `unreachable` |
| direct call `-> Never` | followed by `unreachable` |
| `hang()` | lowers to `call @__valen_hang()` + `unreachable`; no inline loop |
| panic entry point | `noreturn nounwind`, not `memory(none)` (check post-opt too) |
| any function | no `speculatable`, no `nosync` emitted by us (post-opt inference is fine) |

### 7.3 Optimization-positive (FileCheck on `build.opt.ll`)

| Test | Check |
|---|---|
| §4.1 `restrictdse` | two stores (regression) |
| §4.1 variant, callee is Valen fn with abort path | two stores |
| §4.2, §4.3 | one load before loop (regression) |
| **§4.4 store sink** | one store after loop, none inside |
| §4.4 with callee = Valen fn that may block (e.g. calls `recv`) | store still sunk (callee `willreturn`; `!noalias {g}`) |
| §4.13 collatz unused | call deleted |
| §4.13 with result used | call present |
| pure fn with loop, result unused, but fn is `-> Never` | call present |
| `if c { hang() } ; work` | `icmp` and `@__valen_hang` call both present (§6.2) |
| §4.12 bounds-check-then-hang | `icmp` present |

### 7.4 Behavioral, must-hang (differential runner, timeout)

| Test | Expect at `-O0` and `-O3` |
|---|---|
| §4.6 port of #28728 with `-> Never` | hang; no crash |
| §4.10 `main -> Never { setup(); hang() }` | prints setup output, then hangs |
| §4.11 diverge-on-input, `x = 0` | prints "about to hang", hangs |
| §4.12 `check(idx >= len)` | hangs; never reads OOB (use a canary / ASan) |
| §4.15 closure wrapping `hang`, typed `-> ()` | hangs before "after" |
| §4.15 `fn() -> Never` pointer | hangs |
| `-> Never` fn that recurses forever | hangs (or stack-overflows at -O0); no other outcome |
| event loop `-> Never` that only exits via `exit(0)` | runs, exits 0 when told |
| blocking `recv()` on a channel nobody sends to | hangs; output before it identical |

### 7.5 Behavioral, must-abort / wrong-answer-accepted

| Test | Expect |
|---|---|
| bounds-check failure after some output | identical output at both levels, then abort with identical status |
| §4.7 #54049 `bar(0)` | `-O0`: stack overflow or hang; `-O3`: returns some `i32`. Document; don't assert value |
| §4.13 collatz, `-O3` | prints "done" |
| §6.3 poison probe | pre-call output identical at both levels |
| DSE'd store + abort + core dump | core shows old value; documented, not a failure |

### 7.6 Threading

| Test | Expect |
|---|---|
| §4.16 store, lock, other thread reads under lock | reads new value at `-O3` |
| spin-wait on atomic flag set by another thread | terminates |
| store to non-atomic shared without sync | rejected by borrow checker (should be uncompilable) |

### 7.7 Unsafe / FFI

| Test | Expect |
|---|---|
| §4.14 safe version (slice, bounds-checked) | hangs, never faults |
| §4.14 unsafe raw version | hang or segfault both accepted; test documents it |
| FFI `-> Never` that returns (C side) | debug: trap fires (§6.9); release: UB, test only in debug |
| FFI that `longjmp`s, declared `may_unwind` | no `nounwind`; program behaves |
| FFI that `longjmp`s, NOT declared `may_unwind` | documented UB; not tested |

### 7.8 LLVM-version canaries

Hand-written `.ll` files run through `opt -O3` with FileCheck, independent of Valen: §6.1 (inliner merge), §6.2, §6.3, and the `experimental.guard`-shape (a `willreturn` call followed by an `sdiv` with a UB guard — must not fold). These run in CI on every LLVM bump.

### 7.9 Fuzzing (later)

A generator that emits random Valen programs with: nested calls, some `-> Never` leaves, some abort paths, some blocking stubs (a `recv` that never returns), observable prints interleaved. Differential `-O0`/`-O3`, compare output prefix up to hang/abort. Csmith-style. This is the only way to get coverage of the poison-propagation interaction across many shapes.

---

## 8. Spec text to write

The language reference needs a section like this. Draft:

> **Termination and forward progress.**
>
> A function whose return type is `Never` does not return. Calling it transfers control permanently: no expression after the call is evaluated. The implementation preserves this: a program that reaches such a call and would (per the function's definition) run forever, will run forever.
>
> A function with any other return type is *assumed by the implementation to return*. This assumption holds even for functions that, on some executions, block indefinitely (waiting on I/O, a lock, a channel, or a thread) or terminate the program (panic, abort, exit). Consequently:
>
> - Effects that are not observable — writes to memory no other party can read, computations whose results are unused — that precede such a call may be reordered after it or omitted, and will therefore not have occurred if the call does not return.
> - A side-effect-free computation that would not terminate may instead be treated as terminating, with the computation omitted (if its result is unused) or replaced by any value of its result type (if used).
> - All *observable* behavior — output, atomic and volatile accesses, synchronization, and calls to functions with such effects — that precedes the call occurs, in order, before the call, regardless of whether the call returns.
>
> A function that has no path returning a value must be declared `-> Never`; declaring it with another return type is an error.
>
> It is undefined behavior to produce a value of an uninhabited type by any means, including in `unsafe` code, and undefined behavior for a foreign function declared `-> Never` to return.
>
> *Note.* Non-termination of a side-effect-free computation is not considered observable behavior. This matches C++ ([intro.progress]) and differs from Rust, which preserves such non-termination. Debuggers and core dumps of optimized programs may not reflect writes elided under this rule.

The unsafe guide additionally needs: "A raw-pointer load placed after a call may be executed before that call if the call is not `-> Never`. Do not rely on a preceding call hanging to protect a raw access."

---

## 9. References

Grouped by what they're evidence *for*. All primary unless noted.

### The problem (miscompiles from assumed termination)
- rust-lang/rust #28728, "LLVM loop optimization can make safe programs crash" (Ralf Jung, 2015). https://github.com/rust-lang/rust/issues/28728 — the canonical safe-code crash; §4.6.
- rust-lang/rust #54049, infinite recursion returns base-case constant (2018). https://github.com/rust-lang/rust/issues/54049 — §4.7.
- rust-lang/rust #61434, #37747 — `loop {}` without `-> !` eliminated.
- ziglang/zig #1658, "infinite loop optimized away unless function is exported."
- JuliaLang/julia #40009, "Reconsider stance on forward progress guarantees" (Keno Fischer).
- John Regehr, "C Compilers Disprove Fermat's Last Theorem" and "Compilers and Termination Revisited." https://blog.regehr.org/archives/140 , /161
- LLVM #60622, #60637 — C++ infinite-loop over-optimization.

### Rust's resolution (opt out; the intermediate fixes and their cost)
- Mark Rousskov, "Resolving Rust's forward progress guarantees," inside-rust, 2020-03-19. https://blog.rust-lang.org/inside-rust/2020/03/19/terminating-rust/ — cost figures for `llvm.sideeffect`.
- internals.rust-lang.org thread 12003 (same title) — the design debate; notriddle vs Mark_Simulacrum/Ralf Jung; bill_myers's `readnone`+DCE analysis; Ixrec's "compiler might ignore what you wrote ⇒ UB" generalization.
- rust-lang/rust PR #59546 (`-Zinsert-sideeffect`), PR #73561 (LLVM-side attempt, rejected), commit 0517acd (removal), PR #81451 (LLVM 12 upgrade, closes #28728).

### LLVM's attributes
- D62801, "Add willreturn function attribute" (Johannes Doerfert, 2019). https://reviews.llvm.org/D62801 — exact wording; "doesn't have any loop, recursion or terminating function like abort, exit."
- D85393 (`mustprogress` attribute, Atmn Patel, 2020), D86233 (LangRef definition; the "Option 2: opt-in" decision), D86841 (Clang emission), D86844 (LoopDeletion), D88464 (`llvm.loop.mustprogress`). https://reviews.llvm.org/D85393 etc.
- llvm-dev RFC "Introducing the maynotprogress IR attribute" (Sept 2020). https://lists.llvm.org/pipermail/llvm-dev/2020-September/144865.html — Doerfert on why `willreturn` ≠ `mustprogress` (`while(1){atomic_add}` progresses, never returns).
- D65718, "[LangRef] Document forward-progress requirement" (nikic, 2019) — Ralf Jung's "correct compilation must preserve non-termination" statement; rnk's "frontends should opt in" position.
- D20116 (`speculatable`, Matt Arsenault, 2017), D33774 — why `willreturn+readnone+nounwind` is insufficient to speculate calls; the `readnone`-was-misread-as-terminates history.
- D62762 (`nosync`). LLVM discourse "Memory vs synchronization effects."
- llvm/llvm-project PR #69433, "[IR] Don't mark experimental.guard as willreturn" (nikic, 2023) — §4.17.
- LLVM PR #145320 (nikic, 2025) — DCE of calls needs `willreturn` + `nounwind`.
- Nuno Lopes, review comment on D156478 (2023) — the "halt bit" model of abort/exit.
- Alive2: Lopes, Lee, Hur, Liu, Regehr, PLDI 2021. https://users.cs.utah.edu/~regehr/alive2-pldi21.pdf — and its limitation for us (§3.5.iii).

### Rust's `noalias` precedent (attribute bets can go wrong)
- rust-lang/rust #31681, #54878, #84958, #82834, #88325.

### Uninhabited types and unsafe (R5)
- Rust RFC 1216 (`!`), tracking #35121; RFC 1892; PR #54667 (`zeroed::<!>` panics); issues #47412, #61696; PR #38069 fallout (#38889, #38969, #38972).
- Rust-for-Linux, "rust: init: remove impl Zeroable for Infallible" (Laine Taffin Altman, Apr 2024).
- Rust #64219 — indirect `fn() -> !` not marked `noreturn`; #69231 — `noreturn` and the link register.
- Swift SE-0102 (`Never`); PR #4393 (generalize to all uninhabited types "due to multiple bugs").
- Zig `noreturn` type; issues #5728, #13807, #20154, #8631, #17234.

### Standards
- C++ [intro.progress]; N1528 (Boehm, 2010, rationale); C11 §6.8.5p6; P2809R3 (Bastien, C++26 trivial infinite loops); P1494R5 (observable checkpoints); GCC `-ffinite-loops`, Clang `-f[no-]finite-loops`.

### Alternatives considered and not taken
- Koka `div`/`blocking` effects (Leijen, arXiv:1406.2061; koka-lang/koka discussion #489 on `div` unsoundness).
- SPARK `Always_Terminates` / `No_Return`; Frama-C `terminates`.
- F* `Tot`/`Dv`; Idris 2 / Agda / Lean totality — used for proof, never for backend attributes.
- Flix — purity used for JVM DCE.
- CompCert — preserves divergence coinductively (Leroy; the verified-compiler position we are consciously not taking).
- Go spec — infinite loops preserved.

---

## Appendix A — LLVM internals cheat sheet

These are the definitions the argument in §3 rests on. Verify each against the pinned LLVM; line numbers drift.

**`Instruction::mayHaveSideEffects()`** ≈ `mayWriteToMemory() || mayThrow() || !willReturn()`. A call that isn't `willreturn` "has side effects" for the purposes of `wouldInstructionBeTriviallyDead` — this is why unused pure calls survive without `willreturn`.

**`isGuaranteedToTransferExecutionToSuccessor(I)`** for a call ≈ `!I->mayThrow() && I->willReturn()`. Used by: `MustExecute`/LICM (`isGuaranteedToExecute`), `ValueTracking` (poison walks), `SimplifyCFG`, `JumpThreading`, `GVNHoist`, `DivRemPairs`, `FunctionAttrs`.

**`Function::mustProgress()`** ≈ `hasFnAttribute(MustProgress) || hasFnAttribute(WillReturn)`.

**`FunctionAttrs.cpp`, `functionWillReturn(F)`** (approximate):
```
if (F.mustProgress() && F.onlyReadsMemory()) return true;   // pure + progress ⇒ returns
if (!F.hasExactDefinition()) return false;
if (has back-edges) return false;                            // loops: give up
return all calls in F are willreturn;                        // so a noreturn callee ⇒ not willreturn
```
This is why LLVM won't infer `willreturn` on anything with a bounds check, and why we stamp it.

**`LoopDeletion`**: deletes a loop with no observable side effects if the loop has `llvm.loop.mustprogress` or the function `mustProgress()`. A loop with no exit becomes `unreachable`. This is the #28728 and §4.12 mechanism.

**`CodeMoverUtils::isSafeToMoveBefore`** and LICM `canSinkOrHoistInst`: moving across a call requires the call to be `willreturn` *and* `nosync`. Blocking primitives aren't `nosync`.

**`isSafeToSpeculativelyExecute(CallInst)`**: requires the `speculatable` attribute. `willreturn + readnone + nounwind` is not enough (D20116).

**Inliner, `AttributeFuncs::mergeAttributesForInlining`**: CONFIRM handling of `MustProgress`/`WillReturn`/`NoSync`/`NoFree` (§6.1).

**LangRef `willreturn`** (verbatim, as of D62801): "This function attribute indicates that a call of this function will either exhibit undefined behavior or comes back and continues execution at a point in the existing call stack that includes the current invocation. Annotated functions may still raise an exception, i.a., `nounwind` is not implied. If an invocation of an annotated function does not return control back to a point in the call stack, the behavior is undefined."

**LangRef `mustprogress`** (as of D86233): the function is required to make forward progress per C++ [intro.progress]; functions without it are not required to. `willreturn` implies `mustprogress`.

---

## Appendix B — One-paragraph summary for someone who won't read the rest

Valen has exactly two kinds of function: those returning `Never` (emitted `noreturn`; the only way to write an infinite loop or a diverging call) and everything else (emitted `willreturn nounwind`, even if it might block or abort). It is a compile error to declare an inhabited return type on a function with no returning path. The type checker is the only thing that emits `unreachable`, and only after `Never` calls, so `willreturn` is never adjacent to `unreachable` — that single invariant is what prevents the Rust #28728 class of bug. Being wrong about `willreturn` on a blocking or aborting call is harmless because the call's own side effects pin every observable operation in place; only unobservable work can move, and nothing after the call runs to miss it. Never lower `hang()` to an inline loop (use an opaque runtime call), never emit `mustprogress`, never hand-write `nosync`/`speculatable`/`memory(none)`, forbid constructing uninhabited values even in `unsafe`, and re-run the differential test suite on every LLVM bump. Before anything else, check §6.1.
