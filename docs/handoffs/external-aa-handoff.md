# Custom LLVM ExternalAA — handoff

**Status: design + implementation plan. None of this is built yet.** This documents a *future*
optimization. The prerequisite tiers (1–3 below) are separate, cheaper work and are the priority;
this handoff is the fourth, hardest tier. Read the whole "Why" section before touching code — the
whole point is subtle and easy to get wrong in a way that miscompiles.

This is written for a developer new to the aliasing work. It over-explains on purpose.

## One-paragraph summary

Vale's borrow checker knows, precisely, which memory each load/store touches (its "group") and which
groups a call may mutate. LLVM does not, and there is a specific kind of fact we cannot hand it through
any attribute or metadata: *"this opaque call reads the object we passed it, but only **modifies** a
nested child of it."* A **custom LLVM alias analysis** — an object LLVM's optimizer queries during its
own passes — can supply exactly that fact, because the query interface it answers has a read-vs-write
axis that the static annotations lack. Feeding LLVM this lets `GVN`/`LICM`/`DSE`/`MemCpyOpt` keep values
in registers and delete dead stores across otherwise-opaque calls. It runs inside LLVM's pipeline, so it
composes with inlining.

## Where this sits: the four tiers of aliasing speed

We deliver alias-driven speed in tiers, cheapest first. This handoff is Tier 4.

- **Tier 1 — whole-function `noalias` parameter attribute (Phase A).** Landed on `main`. When a parameter
  is the sole reference into its group, `declareFunction` emits LLVM `noalias` on it. See
  `borrow-checker-handoff.md` → "Noalias / restrict codegen".
- **Tier 2 — per-access scoped `!alias.scope`/`!noalias` metadata (Phase B).** In the working tree
  (uncommitted). The checker emits `GroupFactsT` (`src/typing/ast/borrowing_ast.rs`, struct at line 58)
  ground truth; the C++ backend derives the metadata by complement (`noaliasComplement` /
  `attachAccessAliasScope` in `Backend/src/aliasing/aliasing.cpp`). Consumed by LLVM's stock
  `ScopedNoAliasAA`.
- **Tier 3 — cheap refinements (the work happening now).** Two additions, both attribute/metadata only,
  both consumed by stock LLVM AA:
  - *Distinct scopes for distinct allocations* — stop collapsing every group to its root. Two
    pointer-backed sibling collections (`level.tiles[]` vs `level.entities[]`) are physically separate
    heap allocations, so they get distinct scopes and are mutually `!noalias`. See "Distinct allocations"
    below — this is the key correctness idea Tier 4 also relies on.
  - *`readonly`/`memory(read)` from the absence of a `mut` effect* — a call that declares no mutation
    writes nothing, so a load can be hoisted across it even though it may read.
- **Tier 4 — this handoff: a custom `ExternalAA`.** The only route to the one class of fact Tiers 1–3
  cannot express: opaque-call **mod/ref** precision.

**Tiers 1–3 capture the large majority of realistic speedup, and they max out *inlined* call sites** —
because once a callee is inlined, LLVM sees its actual stores (still carrying their scope tags) and needs
no summary. Tier 4's marginal value is specifically at **opaque** (non-inlined) call boundaries: cold
paths, callees too large to inline, and cross-translation-unit calls without LTO. Build Tier 4 only when
profiling of real Vale programs shows opaque-call mod/ref cases costing real time. Until then it is a
documented plan, not pending work.

## Why attributes and metadata cannot express what we want

Two independent axes exist in LLVM, and neither alone covers our case:

1. **Aliasing** — `!alias.scope`/`!noalias` metadata, and the `noalias` pointer attribute. Answers *"do
   these two memory locations overlap?"* It has **no read/write distinction.** `!noalias {S}` on a call
   means "does not **access** S — neither read nor write."
2. **Mod/Ref (memory effects)** — the `memory(...)` function attribute and the per-parameter
   `readonly`/`writeonly` attributes. Has read/write, but only at **whole-pointer or memory-class
   granularity** — never a nested sub-object reached through a pointer.

The case we cannot express with either: we hand a callee `level` (or an `Entity`), and it **reads** the
object to navigate but **only writes a nested child** (say `entity.components[]`, a separately-allocated
buffer). We want a load of a sibling field or a sibling collection to hoist across that call.

- `!noalias {parent}` is false: reaching the child requires loading a pointer field *out of the parent*,
  which is a **read** of the parent's memory — and `!noalias` counts reads. (This is the "touched-hole":
  `!noalias` demands *no access*, and Valen declares only mutations, not reads, so a call handed a parent
  might read anything reachable through it.)
- `readonly %parent` is false: the child is reachable through the parent, and `readonly` is **transitive**
  — writing the child is "writing through the parent."
- There is no attribute for "writes are confined to this one sub-allocation reachable via the pointer,"
  and `initializes((lo,hi))` only describes a byte range of the *direct* pointee, not something behind
  further pointer hops.

So no *static annotation* can say "reads the parent, modifies only the child." That is the gap Tier 4
fills.

## The key insight: the query interface is richer than the annotations

LLVM's optimizer does not read attributes directly. It asks an `AAResults` object questions, and *those*
carry the precision the annotations lack:

- `alias(LocA, LocB) -> {NoAlias, MayAlias, PartialAlias, MustAlias}`
- `getModRefInfo(Call, Loc) -> {NoModRef, Ref, Mod, ModRef}`

`getModRefInfo` **has the read/write split.** A call that reads the parent but writes only the child is
`Ref` (reads, does not modify) for a location in the parent/siblings, and `Mod` for a location in the
child. That is exactly "modifies the child, not the parent" — expressible as a *query answer* even though
it is not expressible as a *tag*. `LICM`/`GVN`/`DSE`/`MemCpyOpt` all drive off these queries, so
answering them precisely is what unlocks the optimization.

A **custom alias analysis** is an object that participates in answering those queries using Vale's group
knowledge. That is the whole idea of Tier 4: not a new annotation, an analysis that *answers* better.

## Distinct allocations — the correctness foundation (shared with Tier 3)

A `[]Tile` field is **not** stored inside the `Level` struct. `level.tiles` is a *pointer* to a separately
allocated buffer. So in memory there are separate, non-overlapping chunks:

```
   Level struct              tiles buffer            entities buffer
   ┌──────────────┐          ┌──────────┐            ┌────────────┐
   │ mana_modifier│          │ Tile 0   │            │ Entity 0   │
   │ tiles   ─────┼─────────►│ Tile 1   │            │ Entity 1   │
   │ entities ────┼───┐      │ ...      │            │ ...        │
   └──────────────┘   │      └──────────┘            └────────────┘
                      └────────────────────────────────► (entities buffer)
```

Consequences:

- A write to a `Tile` element touches the tiles buffer only. It cannot touch the `Level` struct's bytes
  or the entities buffer — they are different allocations. So tile accesses and entity accesses **do not
  alias**, and we may mark them mutually `!noalias`. This is *true*, not a trick.
- The borrow checker's "child group aliases its parent" rule (the `paths_alias` prefix test in
  `src/typing/borrow_checker/grouped_ast.rs`) conflates *reachability* with *memory overlap*. Those
  coincide only for **inline** storage (a `[5]Tile` stored in place, an inline struct field). For a
  **pointer-backed** child (`[]Tile`, `Box<T>`, a runtime-sized array), the child is its own allocation
  and is physically disjoint from the parent and from sibling collections.
- LLVM cannot derive this itself: it sees `%tiles = load ptr, %level` and `%entities = load ptr, %level`,
  two pointers loaded from the same base, and cannot prove they point at disjoint memory. We know each
  owned field owns a distinct allocation; we tell LLVM.

**The distinguishing fact the compiler keys on: is a child group *inline* (bytes within the parent →
aliases the parent) or *indirect* (behind a pointer → its own allocation → disjoint)?** This is a layout
fact the backend already knows. Tier 3 uses it to hand out distinct scopes; Tier 4 uses it to answer
`alias()` with `NoAlias`.

## Where the custom AA runs, and why that place

It registers into the backend's new-pass-manager pipeline in `Backend/src/vale.cpp` (the function around
lines 1218–1260: `PassBuilder PB; PB.registerFunctionAnalyses(FAM); ... MPM =
PB.buildPerModuleDefaultPipeline(opt_level); MPM.run(...)`). The custom analysis is added to the
`AAManager` there, so LLVM's own `GVN`/`LICM`/`DSE`/etc. consult it throughout the pipeline.

This placement is deliberate and is the answer to the phase-ordering question:

- The **transforms** stay LLVM's. We do **not** reimplement `GVN`/`DSE`/`LICM` (in MLIR or anywhere). They
  are mature, and — critically — they must run **after inlining** to catch opportunities inlining exposes.
  A pre-LLVM pass structurally cannot see those.
- Our job is only to **answer alias queries** with group knowledge, at every stage of LLVM's pipeline,
  including post-inline. A custom AA is a query oracle consulted before and after inlining, so it composes
  with inlining for free.

`Backend/` is "core" (`Backend/.claude/CLAUDE.md`): editing `vale.cpp`, `aliasing.cpp`, or adding a new
AA file needs explicit "fire core edits" authorization. Plan for that.

## How the AA gets its facts (and avoids the hard problem)

The naive fear with a custom AA is *pointer provenance*: when the optimizer hands you a mangled pointer
(GEP'd, phi'd, reassociated), how do you know which group it is? Tracing raw pointers back through the
optimizer is the part that makes custom AAs miserable.

We sidestep it: **read the group off metadata, not off the pointer.**

- Every memory access already carries its group as scoped-alias metadata (Tier 2). LLVM threads that
  metadata onto the `MemoryLocation` it hands the AA — `MemoryLocation` has an `AAMDNodes AATags` field
  holding the scope/noalias/tbaa tags, and the inliner **preserves and clones** these across inlining. So
  the AA reads "which group is this location in?" straight off `Loc.AATags`, no pointer tracing.
- For the call side, we attach the call's **mod-set** (which groups it may write) and, if useful, its
  **ref-set** (which it may read) as custom metadata on the call instruction, produced from the checker's
  effect information (`GroupFactsT` plus the `mut(...)` effects). `getModRefInfo` reads that off the
  `CallBase` directly.

So the per-access scope tags *are* the provenance carrier. This is why Tier 2 (scoped metadata on every
access) is a prerequisite: it is not only consumed by stock `ScopedNoAliasAA`, it is also how Tier 4's AA
identifies locations.

## Implementation plan (step by step)

Prerequisites: Tiers 1–3 landed. In particular Tier 2's per-access scope metadata must be present, since
the AA reads it.

1. **Emit per-call effect metadata.** On each `FunctionCall` codegen, attach a custom `MDNode` naming the
   groups the call may write (its mod-set) and, optionally, the groups it may read (ref-set). Source of
   truth: the checker's per-call effect info. Data path mirrors the existing group-facts plumbing:
   `src/typing/borrow_checker` computes it → carried through instantiation → `src/backend_ffi/metal_lowerer.rs`
   forwards it (pure pass-through, no logic — see its CLAUDE.md) → `Backend/src/aliasing/aliasing.cpp`
   attaches the MDNode on the `call`. Reuse the numbering scheme already used for scopes so the group
   identity on a call matches the group identity on accesses.
2. **Write the custom `AAResult`.** A C++ class deriving from LLVM's `AAResultBase`, overriding:
   - `getModRefInfo(const CallBase *Call, const MemoryLocation &Loc, AAQueryInfo &)`: read the group of
     `Loc` from `Loc.AATags` (its scope); read `Call`'s mod-set and ref-set from the call metadata. Return
     `Mod` only if `Loc`'s group is in the mod-set; `Ref` only if in the ref-set; combine into
     `ModRef`/`Ref`/`Mod`/`NoModRef`. **If `Loc` has no group tag, or the call has no effect metadata,
     return the conservative answer (`ModRef`) — never guess.**
   - `alias(const MemoryLocation &A, const MemoryLocation &B, AAQueryInfo &)`: read both groups from their
     tags; return `NoAlias` when they are distinct allocations (distinct scopes that are not in an
     ancestor/descendant relationship — see the inline-vs-indirect rule). Otherwise fall through
     (`MayAlias`) and let the other analyses decide.
   - Everything not covered must fall through to the aggregation (return the most conservative result and
     let `BasicAA`/`ScopedNoAliasAA`/etc. refine). A custom AA only ever *narrows*; it must never widen.
3. **Register it into the pipeline** at `Backend/src/vale.cpp:1228` onward. In the new pass manager this
   means adding the analysis to the `AAManager` (the legacy-PM name for the hook is
   `ExternalAAWrapperPass`; in the new PM you register the analysis and include it in the AA pipeline —
   look at how `PB` builds the default AA pipeline and add ours alongside `BasicAA`/`ScopedNoAliasAA`).
   KGEN's pattern for forcing an analysis into the pipeline around the inliner is a useful reference
   (see "Prior art").
4. **Confirm the metadata survives inlining.** Scoped-alias metadata is cloned by the inliner
   automatically. Verify the *call effect* metadata (step 1) is preserved onto the inlined call sites, or
   that its loss degrades gracefully to conservative (never to unsound).
5. **Tests.** Mirror the existing `restrict*` fixtures in `src/end_to_end_tests/tests/noalias.rs`:
   an opaque C extern that reads a passed object but mutates only a nested child, called between two reads
   of a sibling; assert on `build.opt.ll` that the sibling read is hoisted / the redundant store is DSE'd
   across the call. The barrier extern must be `nounwind` (Vale already stamps this) or the hoist will not
   fire even with correct AA answers.

## Soundness — this is a *trusted* analysis

A custom AA is believed by the optimizer. A wrong `NoAlias` or `NoModRef` is a **miscompile**, not a
missed optimization. This is categorically more dangerous than the attribute/metadata route, where a
forgotten fact only costs speed. Treat the AA's facts with the same rigor as the borrow checker itself:

- **Never guess.** Missing tag, unknown call, unrecognized pattern → return the conservative answer
  (`MayAlias` / `ModRef`). Narrow only when you have a positive, checked fact.
- **The ancestor/descendant rule is load-bearing.** A child that is *inline* in its parent overlaps the
  parent's bytes and **aliases** it; only a *pointer-backed* child is a distinct allocation and disjoint.
  Getting this backwards (marking an inline child disjoint from its parent) miscompiles. Encode the
  inline-vs-indirect distinction explicitly from layout; do not infer it from the group path alone.
- **Distinct owned fields are distinct allocations** only because the type system guarantees it. If Vale
  ever gains shared/unsafe aliasing of owned fields, this assumption breaks; gate on the owned-field
  invariant.

## Prior art

Investigated in the Modular/Mojo "KGEN" tree (external, at `/Volumes/V/LangNotesValen/Mojo/modular/KGEN`;
paths below are in that tree, for reference only):

- **KGEN builds no custom AA and does no alias-driven memory opt in MLIR.** It encodes disjointness once
  as an ownership invariant ("one mutable reference at a time"), lowers it to LLVM `noalias` *attributes*
  (`lib/KGENToLLVM/LowerKGENToLLVM.cpp:537-555`, mapping `ArgConvention` → `noalias`), and lets LLVM's
  stock `AAManager` + `GVN`/`DSE`/`LICM` do the work after inlining. This is strong precedent for our
  Tiers 1–3 route.
- **`pop.noalias_pointer_cast`** (`include/KGEN/POPDialect/POPOps.td:1677`, lowered in
  `lib/KGENToLLVM/LowerPOPToLLVM.cpp:2280`) — an intrinsic cast that launders a pointer through a call
  carrying `noalias` on arg + result, "carries transitively through inlining." This is how you put
  `noalias` on an *interior/loaded* pointer (the plain attribute only goes on params/returns). Useful for
  Tier 3's distinct-allocation buffers where blanket `noalias` is correct.
- **`IPDF`** (`lib/Transforms/IPDF.cpp`) — a per-allocation `READ/WRITE/CAPTURE` effect lattice built by
  querying MLIR's `MemoryEffectOpInterface`. It is the *skeleton* of a mod-set analysis, but unfinished
  (`// TODO: handle calls`), analysis-only, and not scheduled. A starting point if we ever want the
  mod-set computed at the MLIR level.
- **AA injection pattern** (`lib/Compiler/ObjectCompiler/LLVMPassesPipeline.cpp:278/282`) — forcing an
  analysis (`GlobalsAA`) into the pipeline around the inliner and invalidating/recreating the `AAManager`.
  Reference for step 3.
- KGEN does **not** emit `!alias.scope`/`!noalias` scoped metadata or `!tbaa` at all — attributes only.
  Our Tier 2 scoped-metadata approach is *more expressive* than KGEN's for a group model, because it can
  say "may-alias my same-group sibling but disjoint from another group," which blanket `noalias` cannot.

Swift's SIL is the broader precedent for the architecture: do ownership-aware optimization at a high-level
IR where the semantics are explicit, then lower to LLVM and let LLVM do the low-level memory opt. The
cross-call memory precision lands on the LLVM side, fed by facts the high level emitted.

## Decision gate

Build Tier 4 only after Tiers 1–3 ship and profiling of real Vale programs shows opaque-call mod/ref
cases (large non-inlined callees mutating nested sub-objects in hot paths) costing measurable time.
Inlined hot code is already covered by Tiers 1–3. If the profile does not show it, this stays a plan.

## Lessons learned

- **The read/write axis lives in the AA *query* interface (`getModRefInfo` → Mod/Ref), not in the
  annotation vocabulary.** Anything of the form "reads X but modifies only Y" is a custom-AA job, never an
  attribute or metadata job.
- **Attributes/metadata are subtractive and safe-on-omission** (forget one → slower code). **A custom AA
  is trusted** (a wrong answer → miscompile). Do not reach for the AA for something an attribute can carry.
- **"Reachable through" is not "overlaps in memory."** For pointer-backed children the two diverge: the
  child is a separate allocation, physically disjoint from the parent, even though you reach it by loading
  a pointer out of the parent. Inline children are the only ones that truly overlap the parent.
- **Do the transforms in LLVM, not MLIR.** Memory opts must run post-inline; reimplementing them earlier
  can't see what inlining exposes and duplicates mature passes. Feed LLVM facts; let it transform.
- **Per-access scope metadata doubles as provenance.** Reading a location's group off `Loc.AATags` avoids
  tracing mangled pointers — the reason the custom AA is tractable at all.
- **A comparison compiler *not* building something is weak evidence for us if its model differs.** KGEN
  has no custom AA because whole-object exclusivity maps to blanket `noalias`; that says nothing about
  whether *our* finer needs justify one. Decide Tier 4 on our own cost/benefit, not on KGEN's abstention.
- **You *can* put `noalias` on an interior/loaded pointer** — not via the attribute (params/returns only)
  but via an intrinsic cast that carries `noalias` on arg + result (KGEN's `noalias_pointer_cast`).
