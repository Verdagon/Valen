# Speed benchmarks — handoff

The endeavor: prove, then measure, that **canonical Valen is faster than any equivalent safe Rust can
express**, on programs where group borrowing's alias metadata lets LLVM do what Rust's `noalias` cannot.
The implementation plan is `~/.claude/plans/plan-that-all-out-crystalline-sifakis.md` (approved; nothing
in it is started). The test folder is `src/typing/test/rust_interop/speed/`. The alias-metadata machinery
itself is documented in `docs/handoffs/borrow-checker-handoff.md` ("Noalias / restrict codegen").

## The thesis, and where the win actually is

Rust's `noalias` is **provenance-based and parameter-only**: rustc emits it on `&mut T`/`&T` *parameters*,
and LLVM loses it at the first pointer *loaded from memory*. Valen's alias facts are **type-derived and
per-access**: a heap array's elements are their own child group, so element loads/stores carry
`!alias.scope` and a call that cannot reach the group carries `!noalias`, regardless of provenance.

Consequences, all verified on rustc 1.94 (`rustc -O --emit=llvm-ir` on hand-written probes) or by reading
the checker:

- **Parity, not a win** (Rust's NLL-scoped `&mut` expresses the same thing): a sole reference in a loop;
  two same-group references used in *sequential* loops or straight-line code; two references hot in one
  loop (Valen spells the non-aliasing case as two *groups*, `a in g, b in h`, and gets Rust's codegen).
  The handoff line "the block-scoped case Rust cannot express" is true of the *signature* only; the
  program is expressible with sequential re-borrows.
- **The win**: a `&mut self`/`&Vec` method mutating an element of a heap-owned collection across an
  opaque call. Rust reloads the element every iteration (`bump_indirect(w: &mut World)` — the buffer
  pointer is loaded from `*w`; verified: `load i64, ptr %_6` inside the loop after the call). Safe Rust
  can only match by copy-out/write-back or by changing the API to pass `&mut [T]` — both excluded by the
  "same shape" rule. With an occasional **observer** call that legitimately receives the collection,
  copy-out becomes semantically invalid and there is no safe-Rust equivalent even after restructuring.
- **A second, different win** (lookup count, not aliasing): `attack(a, d)` with `a == d` allowed. Every
  single-path safe-Rust equivalent (as-written double lookups, `RefCell`) is slower; only a two-body
  `if a_id == d_id { … } else { get_disjoint_mut … }` restructuring ties, and `get_disjoint_mut` alone
  is not an equivalent (it rejects `a == d`).
- **Rust wins today** in one narrow case: a caller hoisting across an *opaque read-only extern* — Rust
  emits `readonly`, Valen's per-parameter `readonly` is still unimplemented (borrow-checker handoff).

### Fairness rules (the claim is only as good as these)

- **Same shape**: each program written the way its language's programmer would; no copy-out, no
  duplicated bodies, in-loop indexing stays in-loop.
- **Identical accessor and barrier in both arms**: `at<T>(v: &Vec<T>, i: i64) -> &T` (the `i as usize`
  is explicit Rust, the compiler never converts) and an `extern "C"` barrier, both from the same
  `mycrate` rlib. A plain Rust `fn` barrier is may-unwind under `panic=unwind`, which blocks hoisting in
  *both* arms and hides the effect.
- **Identical rustc flags across arms** (`-C opt-level=3 -C lto=off -C codegen-units=1`).
- **Three arms per case, all reported**: Rust baseline; Valen with `suppress_alias_metadata` (isolates
  the metadata's own effect, same compiler); canonical Valen. The Valen−metadata arm is *measured*, not
  engineered to equal Rust — an honest difference there (e.g. a layout tax) is a finding, not a confound.
- **Disclose** that Vale-owned values are interior-mutable from Rust's view (`__ValeOpaque` is
  `!Freeze`), and that Valen writes through a borrow obtained from a shared `&Vec` — that *is* the
  aliasing-permission difference under test.
- **Two tiers.** Tier 1: deterministic IR-shape assertions in the gated suite. Tier 2: an opt-in
  reporter (`[[bin]] valen-speedbench`, per the plan) that prints median/min/spread from interleaved runs
  and asserts nothing. Timing never gates (nondeterminism is a P0). Mac numbers are directional; the
  publishable run is pinned Linux + `hyperfine`.

## The cases

`src/typing/test/rust_interop/speed/`, one `Case` + one driven-and-run test per file, exit code = the
answer. Run: `cargo +rustc-fork test --manifest-path Cargo.toml --lib --features rust_interop typing::test::rust_interop::speed`
(expect 12 passed, 3 ignored; measure before quoting).

| Case | Program shape | Rust equivalent | Verdict | Status |
|---|---|---|---|---|
| 01 sole ref loop | `bump(s &Ship)` | `&mut Ship` | parity | green |
| 02 same group, sequential loops | `a,b in g`, loop a then loop b | two `&mut` / sequential re-borrows | parity | green |
| 03 same group, straight-line | coalesce/DSE shape | two `&mut` | parity | green |
| 04a two refs, one loop, disjoint groups | `a in g, b in h` | two `&mut` | parity | green |
| 04b two refs, one loop, same group, `bump_both(&s, &s)` | may-alias | `&mut [T]` + indices | parity (both reload) | green |
| 05 attack, `a == d` allowed | `Vec<Entity>` via `at` | 4-lookup / `RefCell` / branched | **win** vs single-path | green |
| 06 vec indirection | `bump(ships &Vec<Ship, Global> in g, i i64)` RMW across barrier | `&mut self`+`Vec`, reloads | **win** | green |
| 07 vec indirection + observer | 06 + `observe(ships, i)` at one step | no equivalent (copy-out invalid) | **win** | green |
| 08 flattened VM accumulator | `Vec<Reg>`, `acc = at(regs, 0i64)` RMW + `trace(regs)` | `Vm { regs: Vec }` | **win** | green |
| 09 owned heap member | `f.flagship.fuel` | `Box<Ship>` field | no effect today (member is inline) | green |
| 10 stored borrow in struct | `Holder<g'> { s &Ship in g }` | `&'a mut` field | needs group-generic structs | ignored |
| 11 ref-to-ref | `pp &&Ship in g` | `&mut &mut` | `&p` decays to `&Ship`; not formable as spelled | ignored |
| 12 interface member | `impl Callback for MyCb`, `poll(cb &Callback)` | `Box<dyn Trait>` | backend cannot build the upcast fat pointer (`structs.cpp` `makeInterfaceFatPtrWithoutChecking`) | ignored |
| 13 shared-ownership graph | `Vec<Node>`, `peer i64` links | `Rc<RefCell>` | win in principle | green |
| 14 caller hoists across readonly callee | `total(s)` + `peek(s)` | `&T` readonly | Rust wins until per-param `readonly` | green |

Idioms that make a case compile (learned the hard way, see Lessons): a named `Vec` is `Vec<T, Global>`;
Rust `i64` imports as Valen `i64`, distinct from `int` (Valen's `int` is 32-bit), so every index handed
to `at` is `i64` — literals `0i64`, params `i i64` passed as `__copy_prim(i)`, never arithmetic on them,
struct fields may be `i64`; the return type precedes the effect (`func f<g'>(…) int mut(g) {`); a bare
parameter read yields a borrow, so `at(ships, i)` needs `__copy_prim(i)`.

None of the cases depends on runtime-sized arrays any more; the RSA backend (two asserts:
`peel_all_references` in `Backend/src/metal/types.cpp`, `incrementRSASize` in
`Backend/src/function/expressions/shared/elements.cpp`) stays deferred by standing order and is not on
this endeavor's path.

## Interop facts this rests on

- `at(&Vec<T>) -> &T` followed by a Valen write through the result is **supported and ruled**: a borrow
  returned from Rust into Valen-owned data is a Valen borrow, its mutability comes from the enclosing
  function's `mut(g)`, and the accessor's `&self` receiver leaves it read-write — proposal S25 in
  `docs/architecture/rust-interop-design.md` (Details + a proposed test there). The inbound rule (a Rust
  `&T` *argument* to a Valen function lifts to a no-mut group) is a different direction and still holds.
- At the LLVM level the write is safe because rustc's `readonly` on a `&T` parameter binds only the callee
  for the duration of its call; the one exploitable shape (a Rust frame holding a shared `&T` across the
  Valen write) is exactly the inbound case the checker rejects.
- `__ValeOpaque` carries a real `UnsafeCell<()>` so rustc sees Valen types as `!Freeze` and emits no
  `readonly` on `&ValeType` params (proposal S24 there; landed in exp-4's tree with the red test
  `a_valen_type_is_not_freeze_to_rustc` — the two-marker `(PhantomData<*mut ()>, PhantomPinned)` shape
  alone reads `Freeze = true`). `Vec<Ship>` itself stays `Freeze` (shallow). Do not "simplify" the field to
  `PhantomData<UnsafeCell<()>>`: `core::marker` implements `Freeze` for `PhantomData<T>` and raw pointers.
- An element borrow from `at` has the descendant group `g...`, which shares the `Vec`'s scope number
  (the ellipsis fold in the borrow-checker handoff's Phase B paragraph): still promotable across a no-arg
  barrier; a call handed only the element counts as reaching the whole `Vec`.
- Rust `&mut self` methods import as `mut(g)` = **churn**, which invalidates live element borrows. Right for
  `push` (may reallocate), an over-approximation for `index_mut`/`get_mut`/`swap`. The house answer when
  those are ever imported is a per-method effect *path* declared for the container (`mut(self.buffer[])`
  vs `mut(self)`, `LangNotesValen/Valen/examples/c2-element-vs-structural.md`, via the §24 "group effects
  of Rust methods" annotations) — not a `softmut` tier and not a `preserves` keyword. Not needed for this
  endeavor: `at` is `&Vec` and churns nothing. `DerefMut` is never followed (rust-interop-design S22).

## What must change before numbers are published

- **§2a scope keying** (`compute_group_facts`, `src/typing/borrow_checker/experimental/aliasing_info.rs`):
  scopes are interned by rendered name, so `Rune("g")`, `Local("g")`, `ParamAnonymousGroup("g")` collide
  into one scope and a call can be `!noalias`'d against memory it aliases — a miscompile, not a missed
  diagnostic. Plan slice 7.
- Everything in the plan's slices 1–6 (IR capture at `-O`, one home for the IR-text helpers, the
  `extern "C"` barrier, `at` inlining, the suppress knob, per-case Tier-1 proofs). The metadata has never
  been *observed* in rustc's optimized IR for a Valen body — the plumbing exists (borrow checker on in the
  driven harness, `id_to_aliasing_info` read by `populate_metal_cache`, the suppress option crosses the
  FFI in both modes), but slice 3 is where it becomes a fact.

## U1 — the `-O` placeholder-inlining risk (investigated; not yet guarded)

Every rustc-visible Valen function is a stub `fn` tagged `#[vale::emit_consumer_body]` whose Rust body
is `unreachable!()`; the real body arrives out-of-band under the same symbol. MIR optimization and codegen
read `instance_mir` (the placeholder), not the `per_instance_mir` override (collector-only:
`~/rust/compiler/rustc_monomorphize/src/collector.rs`, `tcx.per_instance_mir(..).unwrap_or_else(|| tcx.instance_mir(..))`).
So at `-O` the MIR inliner could inline `unreachable!()` into `fn main() { exit(__vale_main()) }` or into a
caller of a callback, and the binary would abort without ever calling Valen.

**Why it does not fire today — by accident, not by design.** The inliner's gate is
`~/rust/compiler/rustc_mir_transform/src/inline.rs` `check_codegen_attributes`:
`if !is_generic && !tcx.cross_crate_inlinable(callee) { Err("not exported") }` — no same-crate exemption.
`cross_crate_inline.rs` answers `false` for any un-`#[inline]` fn whose body contains a call, and
`unreachable!()` lowers to a call to `core::panicking::panic`, so the non-generic `__vale_main` is refused.
Callback impls are safe because a generic Rust caller cannot resolve `<C as Trait>::m` at MIR time, and LLVM
never sees a placeholder because the partition override strips those items. **Residual hole:** a
*non-generic* Rust fn calling a callback on a concrete `Self` (`fn drive(c: &MyCb) { c.on_call() }`) has
generic args, skips the gate, and would inline the placeholder; nothing in the tree emits such a caller, a
user's crate could.

**What to do (plan slice 2 records the tripwire):**
- Add the **`cross_crate_inlinable` query override** (local + `extern_queries`) returning `false` for
  `is_vale_codegen_target` items, save-and-delegate otherwise, beside `deduced_param_attrs` in
  `vale_override_queries` (`src/instantiating/rust_interop/mod.rs`). The reference architecture lists this
  override (`vale-rust-interop-architecture.md`, the `provide()` inventory and the never-disk-cached table)
  but never states its return value; our `rust-interop-design.md` override list silently drops it. Sky's
  implementation to copy: `Harmonious/rustc-lang-facade/src/queries/cross_crate_inlinable.rs` (installed in
  `queries/mod.rs`). Proposal S26 in `rust-interop-design.md`.
- **Do not** put `#[inline(never)]` on the roots and **do not** use a `codegen_fn_attrs` override with
  `InlineAttr::Never`: under single-symbol naming the attribute reaches the *real* body's callers too, and
  Sky measured it — 13 inlining fixtures regressed (`Harmonious/rust-interop-architecture.md` §F.17) —
  which is the whole cross-language-inlining perf story. The `cross_crate_inlinable` override sets no LLVM
  attribute and left Sky's inlining matrix intact.
- **Close the test gap**: the only `-O` interop build in the tree,
  `release_build_of_driver_check_links` (`src/typing/test/rust_interop/pipeline_e2e.rs`), asserts a clean
  link and never runs the binary — exactly what the architecture's C7 rule forbids ("the canary must run
  the artifact and check its output"). A sibling that builds a runnable fixture at `--release` and asserts
  the same exit code as at debug is the fence for both the entry and the callback path (Sky's is
  `assert_no_inlined_unreachable_in_main`, a `udf`/`brk` scan of `main`'s disassembly). The one real `-O3`
  run so far is manual: NobiliaV's release build, `NobiliaV/gamedev-wip/docs/handoffs/gamedev-handoff.md`.

**Doc corrections owed** (not done): write `cross_crate_inlinable`'s return value and reason into the
architecture doc's override inventory; add the override to `rust-interop-design.md`'s list (S26); confirm
whether the architecture doc's closure-`Fn::call` and `Future::poll` stub snippets (§14.2, §14.10) really
omit `#[vale::emit_consumer_body]` or are typos; `rust-interop-handoff.md` "Next #3" still says release
`valen build` does not link — it does since the force-`External` fix; the interim `[profile.dev] opt-level`
mitigations there are stale.

## Human-only items the plan needs

- Add the `[[bin]] valen-speedbench` entry to `Cargo.toml` (Guardian's AFEOX shield blocks AI `.toml`
  edits) — requested at plan slice 9.
- "fire core edits" for the one new file `src/typing/test/llvm_ir_text.rs` (+ its `pub mod` line) —
  requested at plan slice 1.
- Run the publishable Tier-2 numbers on pinned Linux with `hyperfine`, after §2a lands.

## Coordination with exp-4 (weave)

exp-4 owns the interop importer (`Vec`/`Deref`, the `at` accessor shape, `__ValeOpaque`). Agreed:
element access stays the `at` helper in the benchmark's own crate until `Deref`-reached `get`/indexing
lands (their next follow-on, to be scoped with the architect and this endeavor together); `i64` index,
bare-borrow return; the `!Freeze` change is theirs and landed (held, uncommitted, in their tree — this
branch is rebased onto their temporary checkpoint, so it is already in our history). The archived
correspondence is under `tmp/messages/` (gitignored). Tell weave when `barrier()`/`#[inline(always)] at`
are added to `fixtures/mycrate.rs` and when the U1 result (below) is in.

## Branch state

This branch is rebased onto `exp-4-wipbx`'s temporary checkpoint; **do not ratchet it to `main` until
that checkpoint is finalized by exp-4**. `git status` shows the uncommitted state (the Phase-B noalias diff,
the `speed/` folder, the design-doc proposal, this handoff); `git log --oneline main..HEAD` shows what
is ahead of `main`. The stash stack is shared across worktrees and contains other sessions' entries — push
with a distinctive label and pop by verified index or apply by SHA.

## Lessons learned

*Accumulates wisdom, not events. One or two sentences per entry.*

- **`rg` skips the gitignored `docs/convos/` when given a single path argument** but searches it with
  multiple path args or `--no-ignore`; a "zero hits repo-wide" claim needs `--no-ignore`.
- **A branch that fails `--features rust_interop` with `ExtraModuleAllocator`/`set_fill_extra_modules_hook`
  errors is behind `main`, not on a bad toolchain** — the fork's hook shape changed and `main` followed;
  rebase.
- **The interop lane is `cargo +rustc-fork … --features rust_interop`**; a driven test is the only place
  the harness is reachable, and Valen bodies get internal linkage, so any IR assertion targets the entry's
  `__vale_main` body, and rustc leaves the IR as `stub.vale_cgu.<random>.rcgu.ll` (never `stub.ll`).
- **`fixtures/stub.rs` cannot host a driven trait-impl case** (it re-exports no `Callback`; the pass-2
  anonymous-substruct projection then fails with `E0405`) — use `fixtures_rust_trait`, whose stub projects
  `MyCb {}`; keep the Valen struct empty to match the ZST projection.
- **Do not engineer a fixture to make a control arm match** (e.g. hoisting the element reference out of
  the loop to hide RSA's header-in-heap layout tax); measure the honest shape and report the difference.
- **RSA is not `Vec`**: RSA keeps size/capacity in the heap block behind one pointer, `Vec` keeps them inline
  in the owner — a real, reportable layout difference, and one more reason the benchmark rides Rust's `Vec`.
- **`get_disjoint_mut` is not an equivalent of a function that admits `a == d`**; the honest single-path
  Rust equivalents are the as-written double lookup and `RefCell`.
- **A no-arg Rust `fn` is not an opaque barrier for optimization tests** unless it is nounwind
  (`extern "C"`, or `panic=abort`); Valen already stamps externs `nounwind`, Rust does not.
- **Register renaming will not do the optimization for the Rust arm, but store-to-load forwarding and
  out-of-order execution shrink the measured gap**; the IR proof is the rigorous claim, the timing is
  illustrative — and a read-only dependent-chain loop shows the gap better than a load-add-store loop.
