# Region / group borrow checker — handoff

The group-based borrow checker: what it catches today, what remains, and the open region/effect design
decisions behind rung 1 and beyond. Its design doc is `src/typing/docs/architecture/borrowing-design.md`
and its roadmap `docs/plans/path-to-borrowing.md`.

There are **two checkers** under `src/typing/borrow_checker/`, sharing one set of canonical types:

- `experimental/` — the reference implementation, green on the whole suite. **AI-editable.**
- `sorcerous/` — the architect's rewrite in progress. **Core**, as are the canonical type files
  `borrow_checker/*.rs` and `borrow_checker/mod.rs`; edits there need "fire core edits". (The macros
  under `src/typing/macros/`, `rust_interop/`, and `src/postparsing/function_scout.rs` are AI-editable.)

## Selecting a checker

`borrow_checker/mod.rs` compiles exactly one of the two, both defining `Compiler::check_function` and
`humanize_borrow_error`. The default build compiles `experimental`; `--features borrow_checker_sorcerous`
compiles `sorcerous`. Flipping the default is two edits: the `cfg`s in `mod.rs` and the feature name
in `Cargo.toml`'s `[features]` (Guardian blocks `Cargo.toml`; the architect disables it for that edit).

Because the default build selects `experimental`, the fire-commit gate runs on it, and the sorcerous
build is exercised by hand:

```
cargo nextest run --manifest-path Cargo.toml --features borrow_checker_sorcerous -E 'test(/borrow_checker/)'
cargo +rustc-fork nextest run --manifest-path Cargo.toml --lib --features "rust_interop borrow_checker_sorcerous" -E 'test(<name>)'
```

Run `cargo check --manifest-path Cargo.toml --lib` first thing in a session: the architect edits the
canonical types while `experimental/` is being worked on, and the mismatch shows up there before
anything else. The IntelliJ run configuration for the interop tests needs `RUSTUP_TOOLCHAIN=rustc-fork`
in its environment; the project toolchain stays stock nightly.

## The canonical types (`borrow_checker/*.rs`)

- `ast_g.rs` — `ExpressionGE`, a full structural mirror of `ExpressionTE`: one `*GE` struct per typed
  node, every field carried over, `'g`-arena children, `result()` in core. `GroupStep` (leaves by value:
  `Rune(IRuneS)`, `ParamAnonymousGroup(IVarNameT)`, `Local(IVarNameT)`), `MutEffectPath { effecting_node_loc,
  steps: &'g [GroupStep] }`, `LocalVariableG`. Call nodes and `WhileGE` carry `mut_effects: &'g [&'g
  MutEffectPath]`.
- `kind_g.rs` — `KindGT` and its payloads. `RuntimeSizedArrayGT.element_type` is a `KindGT` by value;
  `StaticSizedArrayGT.element_type` is still `&'g KindGT`. `StructGT`/`InterfaceGT` hold `&'g
  [ITemplataG]`.
- `templata_g.rs` — `ITemplataG<'s, 't, 'g>`, payloads arena-allocated in `'g`. `KindTemplataG` holds a
  `KindGT`, so template args carry groups; `GroupTemplataG { group: GroupExprG }` is the group a group
  parameter stands for in the current frame.
- `group_expr.rs` — `GroupExprG`, a tree with value leaves and `&'g` bases; `Ellipsis` and `Union` are
  variants. A proposal to flatten it into `GroupPath { root, steps, descendants }` is in the design doc's
  Design Proposals.
- `check_usages_types.rs` — `RefKey` (`Named`/`Held`), `GroupSubtree`, `LocalEntry`. No `Default`
  on the last two by decision; build them explicitly.
- `access_event.rs` — `Read`/`Store`/`Call`/`Marker`, the restrict-region log.
- `borrow_error.rs` — `BorrowErrorKind`; its `humanize()` is a stub. Each checker renders its own
  wording (`experimental/errors.rs`, `sorcerous/errors.rs`), and the core humanizer dispatches through
  `mod.rs`'s feature-gated re-export.

## Experimental: what it is and where it cuts corners

`check_function` (`experimental/check.rs`) runs `groupify_function` (`groupify.rs`), `check_usages`
(`check_usages.rs`), then `calculate_aliasing_info` (`aliasing_info.rs`). Groups for parameters and
callee signatures come from `make_kind_g` in `borrow_types.rs`, which also holds the payload mirrors
(`struct_gt`, `ssa_gt`, `rsa_gt`, `super_kind_gt`, `templata_g`) and `citizen_args_in`, which groupifies
a citizen's template args against their written types so `List<&Header>` gives the inner borrow the
parameter's group. `grouped_ast.rs` holds only what the walk needs beyond the canonical nodes:
`children()`, the flat-path helpers `flatten`/`split_unions`/`paths_alias`, and `JointFact`.

It catches use-after-churn below a churned group (array elements, `a.data[]`), through a returned
reference (`v = get(&a)` with `get` returning `&int in g[]`), across `if` joins and `while` bodies, the
two joint-argument checks, and the producer churn gate (`UndeclaredChurn`). The **roster model** it
implements, and which the design specifies: a `let` of a borrow registers `Named(v)` at the node its
group's flattened path names (`register`/`navigate`, the only place the tree grows); a churn walks its
steps and stamps `invalidated_by` on every entry below a `ChildElements` edge plus every ellipsis entry
on the way down; a use looks the reference up **by key** across the whole tree (`is_use_after_churn`)
and never consults its group. `call_result_kind` requires a written return type on every non-lambda
callee and panics otherwise; only a lambda falls back to the typed return.

Where it cuts corners, each a "not yet" rather than a different design:

- `subst_group_expr` passes an unbound callee rune through unchanged (`unwrap_or(Rune(rune))`).
- `resolve_callee` returns `None` for any callee whose typed name isn't `INameT::Function`, and the call
  is then treated as groupless with no churns.
- `arg_rune_subst` binds runes only off a parameter's outermost `&T in r`; a rune inside `[]&T in g` or
  `Vec<&T in g>` is never bound.
- `substitute_groups` copies a citizen's template args through unsubstituted, so a group inside
  `Vec<&T in g>` in a return type is not rewritten.
- Uses are checked only at call arguments (`arg_ref_use`); `x = *ref` or `ref.hp` after a churn is not
  caught. No test in the suite exercises a non-call use.

## Sorcerous: state and the next steps

`groupify_function.rs` threads a rune-to-templata map per frame (`local_rune_to_templata`), registering
the function's placeholders and its own group parameters as identity (`g → Group(Rune(g))`) up front, and
a `local_to_type_g` record of locals. Implemented arms: `LetNormal` (the local's type is the
initializer's grouped result), `LocalLookup`, `Unlet`, `Discard`, `Return`, `Block`, `Consecutor`,
`ConstantInt/Bool/Float`, `ArgLookup`, `ArrayLength`, `RuntimeSizedArrayLookup`, `FunctionCall`
(`match_types` binds the callee's kind runes through borrows, arrays, citizen args and primitives;
`groupify_effect`/`groupify_group` build churn paths). `check_usages.rs` is mid-conversion to the roster
model. Find the open work with:

```
grep -n "???" src/typing/borrow_checker/sorcerous/*.rs
grep -c "unimplemented!()" src/typing/borrow_checker/sorcerous/*.rs
```

The test ladder, each the next stop after the one before:

1. `test_use_returned_reference_after_churn_rejected` (`test/borrow_checker/use_after_churn_tests.rs`,
   native, no rustc). Its trace on both checkers is the reference point: experimental registers `v` at
   `[Local(arr), ChildElements]` and churns `[Local(arr)]`; sorcerous registers every local at the root
   and reaches `main` with the callee's `Rune(g)` unsubstituted in both `v`'s group and the churn.
2. `a_mut_borrow_aliasing_a_shared_borrow_of_one_local_is_rejected` (`test/rust_interop/cases.rs`,
   runs without builtins): the joint-argument check.
3. `use_after_churn_through_a_rust_borrow_return_is_rejected` (same file): the `g...` return.

What remains, in order of what the ladder hits:

1. **Bind group runes at a call.** `match_types`'s `BorrowRef` arm discards the written region and the
   argument's group; record `rune → Group(GroupTemplataG { group })` there. Then `groupify_group` and
   `groupify_type`'s `BorrowRef` arm look a rune up in the frame's map and treat a miss as a bug. The
   design proposal "One rune-to-templata map per frame resolves kinds and groups alike" states the rule.
   With `KindTemplataG` holding a `KindGT`, substitution into a returned `Vec<&T in g>` must recurse into
   template args.
2. **`check_usages` on the roster.** Register at the `let` by navigating-and-creating along the group's
   path; churn navigates its steps then invalidates below; check `Named(v)` at `LocalLookup`, which
   covers call arguments, derefs, member lookups, `return` and `set` uniformly; at a call, hold each
   borrow-typed argument value under `Held(n)` until the call and check the held keys before it.
   `check_templata_uses` and `check_group_use` have no role in that model.
3. **Churn edge semantics.** A churn stamps ellipsis entries at every node on the way down, and below the
   reached node everything past a `ChildElements` or `Variant` edge; `Member` and `InlineElements` edges
   are walked through. `invalidate_descendant_groups_of` already classifies the edges.
4. **`if` and `while`.** Clone the tree per arm and merge stamps (a diverging arm contributes none);
   pre-apply a loop body's churns before walking it.
5. **Joint arguments and the producer gate**, both reading the callee's parameter runes and `mut(..)`.
6. **Diagnostic wording** matching experimental's, since most tests assert rendered text.

`observe<T, tg'>(x &T in tg)` in the ladder's first test is a fixture change made so that sorcerous meets
a written group before it handles `RegionS::Unspecified`; an unannotated `&T` parameter gets
`ParamAnonymousGroup(x)`, and `RegionS::Held` the same.

## Written return types (landed)

**Rule: only a lambda may lack a written return type; any other `FunctionS.maybe_return_type == None`
is a producer bug.** Producers: the scout writes `void` for a named function with no written return
(`function_scout.rs`, mirroring its rune-side `is_parent_function` check); struct and interface drops
write `void`; a struct constructor writes the struct applied to its generic runes
(`struct_constructor_macro.rs`); an anonymous-interface forwarder writes the forwarded method's return
with runes inherited through `map_runes_in_type_st` (`anonymous_interface_macro.rs`); a Rust-backed
function's by-value, primitive or generic return is written via `value_position_type_st`
(`rust_interop/declarations.rs`). Tests: two in `postparsing/test/post_parser_tests.rs`, five in
`typing/test/written_return_type_tests.rs` (including the all-producers invariant), three tier-1 in
`test/rust_interop/cases.rs`. A borrow return with zero or several reference inputs is written with
`RegionS::Unspecified`; no fixture has one.

## Code versus design

- The Design section's grouped-AST passages name `IExpressionGE`; the code's type is `ExpressionGE`,
  with per-variant structs rather than inline fields.
- The Design section's `ITemplataG` listing has `&'t` payloads; the code's are `&'g`.
- The Details section's "Use" bullet says a use queries the tree at the reference's groups;
  experimental looks up by key, and the roster model above is what sorcerous is converging on.

## Noalias / restrict codegen (separate endeavor)

Turns the checker's aliasing facts into LLVM `restrict`/alias metadata; full RFIGA plan in
`~/.claude/plans/please-plan-out-implementing-fancy-hanrahan.md`, design in `borrowing-design.md`'s
`calculate_aliasing_info` / `Noalias` sections. The benchmark endeavor built on it — the cases, the
fairness rules, the thesis of where Valen beats Rust and where it only ties, and its implementation plan —
is `docs/handoffs/speed-benchmarks-handoff.md`.

**Landed on `main`:**
- **Phase A — the per-parameter `noalias` attribute**, end to end. `calculate_aliasing_info`
  (`aliasing_info.rs`) computes it, carried in side maps (`FunctionAliasingInfoT` keyed by signature, then
  `FunctionAliasingInfoI` keyed by instantiated `IdI` — never on AST nodes), emitted by C++
  `declareFunction` (`function.cpp`) via `Package.paramNoaliasByName` read by `lookupParamNoalias`
  (`boundary.cpp`); test `end_to_end_tests/tests/noalias.rs`.
**Phase B — block-scoped `!alias.scope`/`!noalias`, region-free, per-child scopes (uncommitted; `git status`).**
The checker emits ground truth and the backend derives the metadata by complement; there is no region/span
analysis. `groupify_function` records an ordered access log (`AccessEventG` in `grouped_ast.rs`): each
load/store with the group it touches, each call with the groups its arguments reach. `compute_group_facts`
(`aliasing_info.rs`) turns that into the arena-allocated `FunctionAliasingInfoT<'s, 'x>`
(`ast/borrowing_ast.rs`): **each distinct group by its
full path** — `l`, `l.tiles[]`, `l.foes[]` are separate scopes (first-appearance order, index = per-function
scope id), numbering every parameter group first (even one never accessed, via `intern_group` seeding) then
any further accessed group. The facts are the **unified** `instruction_loc_to_accessed_groups`: each
load/store/call's `LocT` → the set of group indices it accesses — a load/store its single group, a call the
**downward closure** of its arguments' groups (a group whose path has an argument's as a prefix — a call
reaches a group's descendants, never its ancestors). `access_group` (`groupify.rs`) composes an access's
group by walking to the root reference and applying member/`Elements` steps, truncated after the last
`Elements`. An ellipsis group (`g...`, e.g. the borrow a Rust accessor returns into `g`) folds onto its
base for scope numbering (`flatten`, `grouped_ast.rs`): never a wrong `!noalias`, still promotable across a
no-arg call, but a call handed only the descendant counts as reaching the whole group. A heap
(runtime-sized) array's elements are a child group (`element_result` → `Elements`); a
static-sized array is inline, so its element ref folds into the parent group (`ssa_element_result`, no
`Elements`). `FunctionAliasingInfoT<'s, 'x>` and its `group_paths: &'x [GroupIdT<'s, 'x>]` (interned `StrI`
identities, debug-only — the backend needs only `group_paths.len()`) are **arena-allocated, never on the
heap**: `check_function` builds it in the check arena, `function_compiler_core` copies it into the typing
arena, and `HinputsT` holds `&'t FunctionAliasingInfoT<'t>`. Surfaced via `HinputsT::aliasing_info`; unit
tests in `test/borrow_checker/group_facts_tests.rs`.

The **backend derives the metadata by complement**. Instantiation copies the facts into the arena
`FunctionAliasingInfoI<'i>` (unified `ArenaIndexMap` loc→set, `group_count`) on the `id_to_aliasing_info`
carrier. `metal_lowerer` forwards each instruction's accessed-group *set* (and the group count) by `LocI` —
pure pass-through, no logic; `Backend/src/aliasing/aliasing.{h,cpp}` (`attachAccessAliasScope` /
`noaliasComplement`) tags an access `!alias.scope {its groups}` + `!noalias {every other group}` and a call
`!noalias {every group it can't reach}`, building one scope MDNode per index under a per-function domain
(cached on `FunctionState`). The access path is set-valued end to end (uniform with the call path);
`Mutate`/`CopyPrim` carry `std::vector<uint32_t> groupIndices`. `Region::store` (`iregion.h`/`unsafe`/`rcimm`) returns the
store `LLVMValueRef` so the `Mutate` dispatch can tag it; `DerefIE` is intentionally untagged — every
`DerefTE` is a `&&T→&T` decay `groupify` skips, so value loads route through `CopyPrim` (see Lessons).

**A call is `!noalias {g}` only for groups its arguments cannot reach** — not "groups it doesn't mutate."
Valen declares only mutation (`mut(g)`), never reads, and read-only aliasing is legal, so a read-only
callee handed an in-group reference must not be `!noalias`'d; argument-reachability is what keeps it sound.

**Extern calls are `nounwind`.** `declareExternFunction` (`function.cpp`) stamps `nounwind` on every extern
declaration (Vale is `panic=abort`; LLVM can't infer it for a bodiless callee) — the first function-level
attribute in the backend. This is what makes the block-scoped metadata load-bearing *across an opaque
call*; `willreturn` proved unnecessary (see Lessons).

Proofs in `end_to_end_tests/tests/noalias.rs` (fixtures under `src/tests/programs/externs/`):
`restrictcoalesce` (two reads fold to one), `restrictdse` (redundant store dead-store-eliminated),
`restrictloopsole` (a sole reference's field register-promoted across a loop's opaque calls — Rust `&mut`
parity), `restrictloopblock` (two aliasing references each register-promoted in its own loop — a
*signature* Rust cannot write, though the program itself is Rust-expressible with sequential re-borrows,
so it proves parity, not a speed win; see the speed handoff for where the win actually is).

**Remaining work + design-vs-code gaps** — full RFIGA in `~/.claude/plans/plan-it-all-out-humming-moler.md`.
The struct reshape and the test-hardening are **done and match the doc**; the code now carries the doc's
arena `FunctionAliasingInfoT<'s, 'x>` (arena `group_paths`, unified `instruction_loc_to_accessed_groups`)
and `noalias.rs` has on/off optimizer-delta tests via a `suppress_alias_metadata` backend toggle. Still
open:
- **Function-level read-only (`memory(argmem: read)`) is DEFERRED as unsound; per-param `readonly` is the
  plan.** LLVM's `memory(argmem: read)` = `argmemonly` + `readonly` (touches only argmem), but "declares no
  `mut`" doesn't imply argmem-only — a no-`mut` function can allocate, do I/O, or read globals. The sound
  replacement (an unratified proposal in `borrowing-design.md`'s `## Design Proposals`) is a per-parameter
  `readonly` attribute: a borrow param gets `readonly` iff **no declared `mut(...)` path overlaps its group
  path in either direction** (not its own group, nor any ancestor or descendant — a `mut(g.items[])` writes
  a child reachable *through* an `&… in g` parent pointer, so that parent is not `readonly`). The overlap
  test is `paths_alias`. No carrier field exists for this yet (the old `function_read_only` stub is removed);
  the verdict rides next to `param_index_to_noalias`, and implementing it needs the per-param computation
  plus confirming LLVM's `readonly` transitivity for the target version.
- **Per-child scopes at e2e ride Rust's `Vec` through the interop lane, not RSA.** The heap-array case is
  `speed/` cases 06–08 and 13 (`Vec<Ship>` + the `at` accessor); per-child scopes are unit-covered in
  `group_facts_tests.rs`, and the optimized-IR proofs are the speed plan's slices 1–6 (the metadata has not
  yet been observed in rustc's `-O` IR for a Valen body). Note a `!N` metadata resolver can't recover a
  group *index* from the IR — scope MDNodes are anonymous self-referential nodes with no index label — so
  only *relational* scope assertions are ever possible at e2e.
- **§2a below (scope numbers keyed by rendered name) is a miscompile risk**, fixed in the speed plan's
  slice 7 (key by `Vec<GroupStep>`) before any benchmark number is published.
- **§2b below (nested in-group borrows invisible to `param_noalias`) bites the interop lane** once
  `Vec::get`/`Option<&T>` returns are imported (exp-4 will warn before starting that work). The architect
  declined a conservative whole-function `noalias` suppression as a stopgap; the fix is the
  `reachable_groups(&KindGT)` walk.
- **Doc `## Design Proposals`** still holds the unratified "Aliasing info is region-free" and inline/heap
  fold proposals (the code implements both); the architect ratifies when ready.

## Region and effect decisions (landed)

The region/effect rulings live in `docs/plans/path-to-borrowing.md`: groups live only on the declaration
side (`GroupS`), never on the value type; `ITemplataT::Group` is a ceremonial constant, not the algebra
(the G-side `GroupTemplataG` is where the group becomes real); a group is never `mut`/`imm`, condemning
`RegionT::Iso` (and `RegionT` itself, once `BorrowRefT` is emptied) as a fossil; borrow creation computes
rather than checks, but the checker derives a borrow's group by tracing to its anchor, not off a `KindT`;
a borrow-of-claim must carry the claim's `rc.T` mention; effect representation is unsettled (live
candidate a per-group permission map); `not(mut(…))` applies to the whole call; the checker iterates the
finished tree.

**Design index.** The design doc `src/typing/docs/architecture/borrowing-design.md` covers the two phases,
the grouped AST, `make_kind_g`, `GroupSubtree`. The roadmap `docs/plans/path-to-borrowing.md` carries the
ladder (rungs 0-3), the design rulings (regions are inert cargo, a group is an identity not an extent,
invalidation keyed on reach, the two join disciplines, the two seams, quarantine by capability, per-body,
the whole-signature input), and the region/effect rulings.

## Further out

After sorcerous reaches parity: **override effect-matching** (an override's declared `mut(...)` compared
positionally against the abstract method's; a second borrow-checker entry point invoked from
`edge_compiler.rs`, so "fire core edits"; see `borrowing-design.md`'s Overrides ruling and its "Override
effect-matching is a borrow-check" proposal); then **group-generic closures** per
`docs/plans/group-generic-closures-plan.md`, the dominant deferral; then optional/weak-borrow returns,
`Box`/`Variant` child-group sources, the `a | b` union / `rc` grammar, and effect *checking*. The deferred
cases panic in `experimental/borrow_types.rs` (`make_kind_g_groupless`'s borrow arm, `group_anon`,
`templata_g`'s `Group` arm, `group_expr_from_group_s`'s `Local` arm); several messages point at the
closures plan when they are really the weak / group-argument / `in x`-local / `held` deferrals, so
retarget each when picking its feature up. Deferred-feature tests outside `test/borrow_checker/` are
`#[ignore]`d; `grep -rn "#\[ignore" src/typing/test/` finds them. The driven-harness closure failures are
worked around in `migrate.vale` (both `migrate` overloads fall through to `__vbi_panic()` under a
re-enable-when-borrowing-ok VCOORD).

## Open design questions (ours)

- **The effect representation.** A bare `mutates: RegionT` is too narrow (upstream answer 21). Live
  candidate is the per-group permission map; the axes are `held` (destruction), `dangle`/`opaque`
  (dereference), and a possible `softmut` tier — **partly independent flags, not an ordered level.** Needs
  an eager canonical form for the group algebra, since map keys are group expressions.
- **Our clone bound has no effect slot** (upstream answer 25).
- **Provenance / "contributing site."** No longer undercut by ranges — every `ExpressionTE` node now
  carries a `RangeS`, so a post-hoc checker can point at the offending line. `CaseRuneFromImpl { inner_rune }`
  remains the in-tree precedent for canonicalize-the-value-keep-the-origin.
- **"Params get runes, locals get classified" needs amending** — answer 19 says some group parameters can
  be *independent*, so the clean split doesn't hold as stated.
- **`BorrowState`'s shape** — still correctly parked behind its stated trigger.
- **Where a minted anchor's release charge lands**, and whether the found-anchor case admits the
  quiet-window certificate. Both are open *upstream* and both land on the consumer-side claim rules.
- **What the expression `&x` forms at a claim-typed local** — payload borrow by concrete sugar,
  compositional borrow-of-claim, or a one-hop argument coercion. Open upstream, and they have asked for our
  input, since our lowering implicitly picks a horn.

## Decisions only the architect can make

1. **Rung 0, rung 1's joint-argument check, and use-after-churn are landed — the start question is
   settled, do not re-raise it.** The scope is in `docs/plans/path-to-borrowing.md`. Groups live only on
   the declaration side, never on the value type — empty `BorrowRefT` to `{ inner }`, a ceremonial
   `ITemplataT::Group(Default)` constant for the uniform group param, `GroupP`/`GroupS` enums + a
   minimal `mut(g)` clause (`GroupB` is not yet a defined enum — it appears only in doc comments as the
   planned borrow-checker algebra), all read by the borrow checker off the scout `FunctionS` and the
   written `ITypeST` (no side-table struct; `FunctionT<'t>`, `ITypeST` never into `'t`). Groups never flow through
   the solver. The still-open effect-domain calls are decisions 2-3 below.
2. **Where does mutability live?** design-1 puts it on the **group**, via signature effect clauses (`func
   heal(e: &Entity in g) mut(g)`), and calls that its one departure from Rust (*"there is no `&mut`"*,
   design-1:361). We dropped `&mut` and never added effect clauses, so mutability currently lives
   **nowhere**. Sits *behind* rung 0, not beside it, since effect targets are group expressions. It is
   bigger than "add effect clauses": `mut(g)` / `mut(g.tiles[])` / `mut(E)` with `E: Effects` / `mut(())`,
   the subtractive `not(mut(…))` forms, the deep-effect rule, parameter shorthands that desugar to **fresh
   anonymous group params instantiated per call site**, plus solve-order pins (design-1:1235 — no negative
   bound discharged against less than `E`'s full solution; re-check on widening). That is a **second solver
   domain** (rung 1) alongside types, i.e. `ITemplataT::Effect` — distinct from the ceremonial group-param
   constant `ITemplataT::Group`, since groups themselves never flow through the solver.
3. **Effect vocabulary staging** — full set or `mut(g)` first? And is effect *inference* in scope
   initially? (Ruled: named functions **declare and are checked**, so no call-graph fixpoint; closures
   genuinely infer, and a *recursive closure* is an unaddressed gap.)
4. **Flatten `GroupExprG` into `GroupPath`?** Proposed, not ratified; see Design Proposals.

## Waiting on upstream — exactly four

(1) Whether **`mut(E)`'s `E` is a group or its own sort** — they carry our framing of it, with
attribution, and it is genuinely unruled. (2) Where a **minted anchor's release charge** lands. (3)
Whether the **found-anchor** case admits the quiet-window certificate. (4) The **by-value-claim drop
fix** — `x T` by value at a claim-typed `T` decs the claim at scope end, possibly to zero, under an
*empty* effect clause, because the drop-side generated bound carries no effect half where the clone side
does; until their fix lands, the drop path must not assume purity there. Items 2 and 3 land on the
consumer-side claim rules and are the only upstream items any near-term work touches. They have also asked
for **our** input on the `&x`-at-a-claim-place question above.

## Lessons learned

*Accumulates wisdom, not events. One or two sentences per entry; prune what nobody can act on.*

- **The bookkeeping is a roster of references, not a timeline.** Do not model a reference as carrying
  its origin time to compare against a group's last churn; a reference is an entry placed once at its
  group's node, stamped by any churn that passes over it, and read by key at use. Control flow then
  costs a tree clone per `if` arm and a pre-applied churn set per `while`, not path reasoning.
- **Group invalidation is not Rust's exclusion; do not model the borrow as a lock.** A borrow constrains
  nobody. A *destructive* op on an **ancestor** group invalidates references into its **child** groups
  (downward only); a plain member write, a sibling, or mutating the reference's own contents invalidates
  nothing. The destroy is the aggressor and the borrow is the victim — the reverse of a Rust `&mut`.
- **A group is dataflow at a `let`, declaration at a parameter.** `ref_a = &slot` gets its group from the
  initializer's grouped result, never from a written type (there is none); only a parameter or a callee
  signature reads a written `in g`. A checker that derives a local's type from `var.tyype` panics on the
  first borrow-typed local.
- **Only a lambda may lack a written return type.** The scout writes `void` for an omitted return, the
  macros write theirs, the interop synthesizer writes value-position types; a `None` anywhere else is a
  producer bug, and experimental's `call_result_kind` panics on it rather than falling back.
- **A definition's own placeholders cannot be looked up in the map being built from them.** Registering
  generic parameters is a self-referential loop if the placeholder arm resolves through the map; the
  entry for a definition's `T` is the placeholder itself (and for a group `g`, `Rune(g)` as identity).
- **Forwarders are registered in the postparsed store under their method's template name**, with the
  substruct's init steps (`anonymous_interface_macro.rs` ~line 114), not under their own
  `ForwarderFunctionTemplate` id; `get_function_template` + `peek_postparsed_function` misses them.
- **`run_case` compiles the builtins in, and the builtins contain generic functions.** A tier-1 test
  meant to exercise only its own program uses `run_case_without_builtins`; a test that passes without
  builtins may be passing only because no generic function reached the checker.
- **`cargo test --lib` never rebuilds the `[[bin]]`s.** `pipeline_e2e` shells out to the on-disk
  `valenc-rs`; a stale binary fails with a SIGBUS that looks like a real regression. Rebuild the bins first.
- **Fixture traps.** A generic struct needs `where func drop(T)void` and a struct type argument, since
  the bare test map has no `drop(int)`; a lambda body `{ i }` yields `&int`, so return a literal; an
  `int` written in a signature is a zero-arg `Call(Name(int), [])`, the same shape as a citizen.
- **`...` is the descendant group operator (`&T in g...`), not a comment.** Three dots lex as three `.`
  symbols, consumed only by `parse_group` (`templex_parser.rs`) in group position; do not resurrect
  `...`-as-comment.
- **A use-after-churn needs a child group, and an inline-only plain struct forms none.** A child group
  comes only from an independently-destroyable owned thing — a collection/array element, a `Box` pointee, a
  `Variant`/interface payload — never an inline scalar or struct field. So `struct Fleet { flagship Ship; }`
  has nothing a churn can dangle; a rung-2 test needs a runtime-sized-array element or the like.
- **In a borrow-checker fixture, a move is `^local` (prefix), and arithmetic on a borrowed member does not
  read out.** `f(&x, x)` does not move `x` (bare `x` yields a borrow, rejected against an owned param) and
  `x^` does not parse (postfix `^` unshipped); write `^x`. `set a.hp = a.hp + 1` on an `&Entity` member
  fails with `+(&i32, &i32)` not found, so fixtures use literal member writes.
- **To split a class of derivation failures by root cause, panic on the bad state and census the panic's
  caller frame.** Grepping a failing suite's backtraces for the panicking helper partitions the failures
  in one run; with a print at the arm's entry (`fire core prints`), a single run also names the exact
  parameter or call that reached it.
- **A runtime-sized-array local can now drop cleanly** via a closure-free `DropFunctor<T>` in
  `arrays.vale`; do not resurrect the closure-based array drop (closures are not working yet).
- **Do not mirror a foreign reference implementation's structure as a template.** Copying Polonius's
  file/struct layout would import loans/origins/constraints — abstractions for jobs (region inference,
  exclusivity) group borrowing does not have; write a small own-shape layering doc instead.
- **`noalias` is only violated on modification, so read-only aliasing into distinct groups stays legal.**
  The existing mutation-gated aliasing check is already exactly the precondition `noalias` needs; do not
  tighten it to reject read-only aliasing.
- **`!alias.scope` is inert on its own — it creates a no-alias fact only paired with a `!noalias` naming
  the same scope on another instruction.** So tag every access with its group uniformly, region-free; the
  only load-bearing content is which groups a *call* cannot reach. Do not resurrect the sole-reference
  "restrict region" span-finding: LLVM never moves a memory op across a may-aliasing one, so the region was
  only ever the maximal reorderable set LLVM already computes — it suppressed inert tags and nothing more.
- **A call is `!noalias {g}` only for groups its arguments cannot reach — not groups it doesn't mutate.**
  Valen declares only mutation (`mut(g)`), never reads, and read-only aliasing is legal (design-1 `@FIAZ`);
  a read-only callee handed an in-group borrow, if `!noalias {g}`'d off the mutation clause, would let LLVM
  delete a write it observes. A call reaches a group only through a reference it is handed.
- **Carry per-function backend facts in side maps keyed by function id, never as fields on the typing/
  instantiated AST or the metal AST.** Mirror `extern_abi`/`struct_layouts` (signature → `IdI` →
  `Package.paramNoaliasByName`); presence in the map is the "analyzed" signal (absent = not analyzed),
  and codegen asserts a present entry's length matches the param count rather than silently tolerating a
  short/empty one.
- **Record per-access group facts in `groupify`, crunch them in `calculate_aliasing_info`.** `groupify`
  is the one place holding the typed node and its derived group at once; an access log emitted there beats
  re-walking the grouped tree. (`access_group` returns just the group — the reference's name is not needed.
  Group facts key on the group's **full path**, so `l.tiles[]` and `l.foes[]` are distinct scopes.)
- **An access's group must be the reference's *pointed-into* group, not the operand's touched group.**
  Params are materialized as locals, so `a.fuel`'s touched group roots at the slot `Local(a)`, making `a`
  and `b` (both `&Ship in g`) look like different groups; walking to the root reference and taking *its*
  group reports `g` for both. `access_group` starts from that root group, re-applies the member/`Elements`
  steps of the access chain, and truncates after the last `Elements` — so `a.fuel` reports `g` (no
  `Elements` → root) while `lvl.tiles[0]` reports `l.tiles[]`. Skip a deref whose result is still a
  reference — that is pointer plumbing touching the slot, not a load into the object group.
- **A scope is a group's full path; only `Elements` steps create a new heap region.** A runtime-sized
  array's elements are a separately-allocated child group (`element_result` → `Elements`); a static-sized
  array is inline, so its elements fold into the parent (`ssa_element_result`, no `Elements`), exactly like
  an inline struct member. Member steps qualify *which* `Elements` (tiles[] vs foes[]) but trailing members
  after the last `Elements` share that region, so they truncate away.
- **Static-sized-array element refs are supported; do not conclude otherwise from a parse failure.** The
  borrow-checker unit harness (`compiler_test_compilation`) rejects explicit `[#N]T` *type annotations*
  (`BadTypeExpression`), but construction `[#](…)` and element borrows parse, and full SSA works in the
  integration/e2e harness. In a unit fixture use an inferred SSA local: `arr = [#](…); b = &arr; &b[0]`.
- **Do not conclude a bound ref to an inline member/array-header is scoped like the direct access — it
  isn't, yet.** `access_group`'s no-`Elements` branch truncates to the root reference's *whole* group, so
  `b = &lvl.tiles` then accessing `b` reports `l.tiles` while direct `lvl.tiles` reports `l` — a latent
  mis-`!noalias`. The fix is to truncate to the anchor step; unapplied because no current fixture can bind
  such a ref (needs its own RED first).
- **`LocT::from_lid(interner, call_location)` mints a collision-free `LocT` where none is threaded in.**
  A LID path never contains a `0` and a typing-conjured `LocT` always does, so a from-LID location can't
  clash with the threaded ones — useful at sites like closure-capture that have a `call_location` but no
  `loct` in scope.
- **An extern call site is a tracked `FunctionCall`, not an `ExternFunctionCall`.** Every extern (C and
  rust_interop alike) is called through a `make_extern_function` wrapper; the untracked `ExternFunctionCall`
  (no `loct`) lives only inside that wrapper's body, so do not conclude the analysis misses extern calls —
  it records them, and a no-argument extern reaches no group, so it is `!noalias` against every group.
- **To prove alias metadata is load-bearing, the call between the accesses must be an opaque C extern
  (declare-only, no LTO).** A Vale callee inlines or is inferred `readnone`, so the load coalesces with or
  without the metadata and the test goes vacuous; confirm load-bearingness by suppressing just the emission
  and watching the `build.opt.ll` fold disappear.
- **`BackendMode::Interop` never optimizes or writes `build.opt.ll`** — rustc owns optimization after Vale
  emits into the borrowed module — so a `rust_interop` test cannot assert on optimized IR without new
  driven-harness plumbing (`--emit=llvm-ir` on rustc, not Vale's `print_llvmir`).
- **What blocks optimizing across an opaque call is the call's *unwind* possibility, not its may-not-return
  one.** LLVM keeps a store/reload around a call whose successor isn't guaranteed reached; the observable
  half is unwinding (a handler up the stack sees the old value). Vale is `panic=abort`, so marking externs
  `nounwind` (in `declareExternFunction`) is the fix and is sound by the execution model — do not reach for
  `willreturn` (Rust doesn't mark opaque externs `willreturn` either, and it was needless here).
- **Value loads route through `CopyPrim`, never `Deref`.** `Deref` is decay-only (`&&T→&T`) and `groupify`
  skips it; a raw pointer is treated as a primitive, so reading one out of a `&Vec` is a `CopyPrim`. If
  `CopyPrim`/`Deref` ever merge into one value-load node, keep `groupify`'s "skip a load whose result is
  still a reference" gate, and first reconcile the two `is_primitive` definitions (`KindT::is_primitive`
  refs→false vs `Compiler::is_primitive` refs→true, flagged unsettled in `compiler.rs`).
- **Function-level LLVM attributes print as `attributes #N = { … }` groups referenced by `#N` on the
  declare/define line, not inline** (parameter attrs like `noalias` are inline). An IR test asserting
  `nounwind`/`willreturn` must resolve `#N` and read that group, not string-match the declare line.
- **`memory(argmem: read)` is unsound as a "no-`mut`" attribute.** In LLVM it means `argmemonly` +
  `readonly` — the function touches *only* argument memory. "Declares no `mut`" is far weaker: a no-`mut`
  function can still allocate, do I/O, or read globals. Do not emit it off `function_read_only`; the sound
  form of "doesn't write args" is per-parameter `readonly` on borrow params in non-`mut` groups.
- **Alias-scope MDNodes carry no group-index label** — they are anonymous self-referential nodes under a
  per-function domain. So an IR test can assert *relational* scope facts (two accesses share/differ; a
  call's `!noalias` includes an access's scope) but never "this access is scope 2." Absolute-index checks
  belong in the checker's `group_facts` unit tests, not e2e.
- **The unified `instruction_loc_to_accessed_groups` map is untagged** (no load/store-vs-call kind) — the
  backend recovers the kind from the metal node type, but a unit test seeing only the carrier cannot tell a
  call's reach set from an access into the same group. Test such call-reach properties at e2e (does the call
  get `!noalias`?), not on the carrier.
- **The metal `Lowerer` keeps an owned per-function aliasing snapshot on purpose.** Making it hold the
  arena `&'i FunctionAliasingInfoI` collides with the method-level `'i` its ~15 methods already declare;
  the durable carrier on `HinputsI` is arena, and only the lowerer's transient scratch is owned.

# Borrow checker architecture feedback

Scope: `src/typing/docs/architecture/borrowing-design.md` and every file in `src/typing/borrow_checker/`, read in full, plus the call site in `function_compiler_core.rs`, `borrowing_ast.rs`, the scout `GroupS`/`RegionS`/`EffectS` types, and the test directory.

Headline: the three-phase shape is sound and the code is readable, but (1) phase 2 only checks reference uses at call arguments, (2) moves and drops never invalidate anything, (3) phase 3 keys scope numbers by a rendered string and only looks at outermost borrows, so it can hand LLVM wrong `noalias` facts, and (4) the grouped AST puts `Box`/`Vec` inside a bumpalo arena, which the design forbids and which leaks. Below, grouped as soundness, backend-contract, design drift, and structure.

## 1. Soundness gaps in check_usages

**1a. Uses are only checked at call arguments.** `check_ge` looks for a stale reference only via `arg_ref_use` and `held_register_of` inside the `FunctionCall` arm (src/typing/borrow_checker/check_usages.rs:114-126). Every other consumer of a reference falls into the `other` arm, which just recurses. So `set elem.hp = 3` (a `Mutate`), `x = elem.hp` (`CopyPrim`/`MemberLookup`/`Deref`), `return elem`, and `w = elem` (`LetNormal`) never hit `is_use_after_churn`. Recommend: check at the `LocalLookup` leaf (and at `Unlet`), since every use bottoms out there, and drop the call-arm special case for named locals. Repro to add as a failing test: element ref, churn, then `set ref.hp = 1`.

**1b. Moving or dropping a local invalidates nothing.** `Unlet` produces a result type and nothing else (src/typing/borrow_checker/groupify.rs:102-105); `check_ge` has no `Unlet` arm. `r = &v[0]; drop(^v); print(r)` passes: the only move check is `BorrowIntoMoved`, which looks at sibling arguments of one call. Same for `set v = Vec()` on a `Vec` local (destroys the old buffer; `Mutate` only logs an access, groupify.rs:127-134) and for the intrinsic `PushRuntimeSizedArray` / `PopRuntimeSizedArray` / `DestroyRuntimeSizedArray` nodes. The design's vocabulary has "churn" (kill independently-destructible descendants) but no "kill" (the group itself is gone, so references *to* it die too). Recommend a second invalidation event, `Kill(path)`, emitted by `Unlet`, `Destroy*`, and a `Mutate` whose destination is an owning place, that stamps `locals` at the node as well as beneath it.

**1c. Interface and extern calls carry no effects.** `InterfaceFunctionCall` and `ExternFunctionCall` get a groupless result, no `mut_effects`, no `Call` access event, and no joint-fact check (groupify.rs:190-197). A virtual call to an abstract method declared `mut(g)` churns nothing. Design proposal S4 says override effects must match the abstract method, which only pays off if the abstract signature drives churn here.

**1d. Unresolvable callee silently means "no effects".** `resolve_callee` returns `None` for any non-`INameT::Function` callee, and both groupify.rs:209-212 and check_usages.rs:176-178 then treat the call as pure and un-aliased. That is a fallback in the approach-review sense: it hides every lambda call from the checker. Either panic (a deferred case, like the others) or model it conservatively as "churn everything reachable through the arguments".

**1e. Nested runes are never bound, then leak across frames.** `arg_rune_subst` binds only a parameter's *outermost* `&T in g` rune (groupify.rs:636-653). A rune that appears only nested, as in `x &Opt<&Ship in g> in d`, stays unbound, and `subst_group_expr` then keeps `Rune(g)` in the callee's namespace (borrow_types.rs:490-491, already marked VLOOOOK). That foreign rune lands in the caller's tree, where `is_param_rooted` reads it as one of the caller's own parameter groups, producing either a spurious `UndeclaredChurn` or a churn of the wrong node. Recommend binding by structural unification of the parameter's `KindGT` against the argument's `KindGT`, and treating an unbound rune as an error, not a fallback.

**1f. The design's headline aliasing example is not checkable.** borrowing-design.md:404-413 rejects `grow(&v, &v)` for `func grow(a &Vec<int>, b &Vec<int>) mut(a)`. `joint_facts` requires both parameters to have a *rune* group (`param_group_rune`) and the effect to have a rune root (`effect_root_rune`), so anonymous-group parameters are skipped (check_usages.rs:212-241). And `mut(a)` scouts to `GroupS::Local`, which `group_expr_from_group_s` panics on (borrow_types.rs:464-467). Either the design's desugaring ("every unannotated borrow param is an implicit rune") should be done for real in `make_kind_g`, so `ParamAnonymousGroup` participates in the joint check, or the doc example should change.

**1g. Nested unannotated borrows get the parameter's anonymous group.** `make_kind_g` threads the same `param_name` into `inner` (borrow_types.rs:202-213), so `&Opt<&Ship>` becomes `&Opt<&Ship in anon(x)> in anon(x)`, which is exactly the shape borrowing-design.md:282 calls "Bad". Pass `None` when recursing into the referent so the nested case is the explicit deferred error it is meant to be.

**1h. `NotMut` is parsed and ignored.** `EffectS::NotMut` exists (src/postparsing/rules/types.rs:171-174) and nothing in the checker reads it. Implement or delete.

**1i. `held` has two policies in two places.** borrow_types.rs:210 maps `RegionS::Held` to the parameter's anonymous group "conservatively"; aliasing_info.rs:44-46 separately excludes it from `noalias`. One place should own what `held` means.

## 2. Backend-contract bugs in calculate_aliasing_info

These matter more than the section 1 items because a wrong `!noalias` is a miscompile, not a missed diagnostic.

**2a. Scope numbers are keyed by a rendered string.** `compute_group_facts` interns groups by `group_name(...)` (aliasing_info.rs:119, 137, 197-201). `Rune("g")`, `Local("g")`, and `ParamAnonymousGroup("g")` all render as `"g"`; two unnamed runes both render as `"?"` (aliasing_info.rs:212). Colliding groups get one scope number, so LLVM is told two distinct allocations are the same one, and the complement gives `!noalias` on accesses that do alias. Key by `Vec<GroupStep>` (already `Eq + Hash`) and keep the string for the debug `GroupIdT` only.

**2b. Only outermost borrows count as reach.** A `Call` event records `borrowref_group(a.result())` per argument (groupify.rs:213-215), and `param_noalias` uses only each parameter's outer borrow path (aliasing_info.rs:41-63). An owned argument or parameter `Opt<&Ship in g>` reaches `g` but is invisible to both, so a sibling `y &Ship in g` is declared `noalias`, and a call receiving the `Opt` gets `!noalias g`. The design's own `readonly` section (borrowing-design.md:598-613) states the right rule, "nothing *reachable*", but the code applies it nowhere. Recommend one `reachable_groups(&KindGT) -> Vec<Vec<GroupStep>>` that walks nested borrows, used by the `Call` event, `param_noalias`, and a future `readonly`.

**2c. Phase 2 and phase 3 disagree on what a `Member` step is.** In `check_usages`, `Member` is a real tree node that `mut(g.items)` and `in x.items` navigate to. In `access_group`, trailing members are truncated to the last `Elements` or the root (groupify.rs:505-511), because a member is the same allocation as its parent. Both are individually right, but they share the `GroupStep` type and the word "group", so a reader (and the next AI session) will assume they mean the same thing. Name them: "ownership path" for phase 2, "allocation" for phase 3, and make the fold a named function that phase 3 applies at the boundary.

**2d. `readonly` is designed, documented, and absent.** borrowing-design.md:538 and 795-797 describe `param_index_to_readonly`; `FunctionAliasingInfoT` in src/typing/ast/borrowing_ast.rs has no such field. Either implement it (it falls out of 2b's `reachable_groups` plus `declared_mut`) or cut it from the doc.

**2e. Possible duplicate `LocT` keys.** `entries.sort_by` is stable and nothing asserts key uniqueness (aliasing_info.rs:174). If two events ever log the same `loct`, the backend map has two rows for one instruction. Assert uniqueness in `compute_group_facts`.

## 3. Design doc drift

The doc is marked as the absolute source (design-implement skill), but it no longer matches the code in load-bearing ways. Someone implementing against it today would rebuild the wrong thing.

- **Arena rule violated.** borrowing-design.md:67 says "no Vec in it, no Box in it, use TFITCX". `KindGT` boxes every nested kind and `Vec`s template args (borrow_types.rs:79-119); `IExpressionGE::While` and `FunctionCall` hold `Vec<MutEffectPath>` and `LetNormal.bind` holds a boxed `GroupExprG` (grouped_ast.rs:86-143). Nodes are placed in the bump arena with `arena.alloc` (groupify.rs:71 and throughout), and bumpalo never runs destructors, so every function's grouped tree leaks its heap parts when `check_arena` drops (function_compiler_core.rs:358-364). This is both a doc violation and a real leak. Also, every `child.result().clone()` (groupify.rs:74-75, 95, 116, 146, 156) is a deep heap copy where the design intended a pointer copy.
- **Signatures differ.** Doc `check_function` (borrowing-design.md:50-61) has groupify returning just the body and `check_usages(coutputs, body)`; code has groupify returning `(body, access_log)` and `check_usages(coutputs, function_s, body)` (check.rs:33-36), plus an undocumented phase 0, `check_return_group`.
- **Step vocabulary differs.** Doc `GroupStep` has `ChildElements`, `InlineElements`, `Variant` (borrowing-design.md:135-145); code has `Elements` only (grouped_ast.rs:25-31). Doc proposal S2 says inline elements fold into the parent, which contradicts `InlineElements` existing at all. The doc's own test-case section uses `Elements(Local("map"))` (borrowing-design.md:842). Pick one vocabulary.
- **Node shape differs.** Doc: "All IExpressionGE variants hold expression structs, just like IExpressionTE" (borrowing-design.md:125). Code: inline struct variants, 47 of them, with hand-written `result()` and `children()` (grouped_ast.rs:181-296).
- **S4 has no code.** No override-effect matching exists in `borrow_checker/` (only `rust_interop/stub_gen.rs` reads override effects). The "second entry point" does not exist.
- **Joint facts are not on the node.** grouped_ast.rs:7-8 and 134-137 say a call node "carries its joint-argument facts"; `JointFact` is defined there but computed on the fly in check_usages.rs:170-244 and never stored.
- **"Only public method is `check_function`"** (borrowing-design.md:24), but `groupify_function`, `check_usages`, `make_kind_g`, `substitute_groups` are `pub`.

## 4. Structural recommendations

**4a. Five representations of a group path.** `GroupS` (scout), `GroupExprG` (tree with Union/Ellipsis), `GroupStep` path (flat), `GroupIdStepT` (arena/debug, exists only to drop the `'t` lifetime), and the `String` key in phase 3. Each conversion is a place to disagree (2a, 2c). Recommend two: `GroupExprG` for what a *type* says, and one flat `GroupPath<'s,'t>` for everything the checker computes with, rendered to a debug string only at the `FunctionAliasingInfoT` boundary.

**4b. The grouped AST is neither self-sufficient nor thin.** Phase 2 reads argument identities, ranges, moves, and place roots off the *typed* node through the `call: &'t FunctionCallTE` back-pointer (grouped_ast.rs:142; check_usages.rs:107, 115, 179-190), re-resolves the callee that groupify already resolved (check_usages.rs:176, needing `coutputs` for nothing else), and re-flattens groups that groupify already flattened. Phase 3 never reads the tree at all (it uses the side-channel `access_log`). Of 47 mirrored variants, four carry checker payload. Two ways out:

  - *Pragmatic:* keep the mirror, but make it complete: store the resolved callee (or a `SignatureG`, see 4c), the argument use list, the move list, and ranges on the call node; drop the `call` back-pointer and `coutputs` from phase 2; remove `LetNormal.bind` (it duplicates `expr.result()`). Temporary: the 47-variant boilerplate stays.
  - *Principled:* drop `IExpressionGE`. Have phase 1 walk the typed AST and emit side tables keyed by `LocT` (result `KindGT` where a node yields a borrow, churns per call, binds per let, accesses per load/store), all arena-allocated with `&'g` slices per the design's TFITCX rule. Phases 2 and 3 walk the typed AST and consult the tables. This deletes `result()`, `children()`, the mirror enum, and the leak, and makes "add a node to `ExpressionTE`" a no-op for the checker unless it matters.

**4c. Four readers of the scout signature.** `arg_rune_subst`, `call_mut_effects`, `param_group_rune`, `effect_root_rune`, and `declared_mut` each re-interpret `FunctionS.params` / `.effects` slightly differently (groupify.rs:424-440, 603-622, 636-653; check_usages.rs:71-79, 212-220). Build one `SignatureG { params: Vec<KindGT>, mut_effects: Vec<GroupExprG>, ret: KindGT }` per function via `make_kind_g`, once, and have substitution, the joint check, the producer gate, and phase 3 all read it. This also fixes 1e (binding becomes structural) and 1f (anonymous groups become first-class).

**4d. Make the checker a struct, not `impl Compiler`.** Every phase threads `coutputs, function_s, function_t, arena, tree, next_held, declared_mut` by hand (check_usages.rs:85-92). A `BorrowChecker<'s,'t,'g> { compiler, coutputs, function_s, function_t, arena }` with phase methods removes the argument plumbing and makes the "pure" claim checkable: the only `&mut` state would be the phase-local `GCtx` and `GroupSubtree`. Related: `resolve_callee` interns an `IdT` into the *typing* arena per call node (groupify.rs:454-458), which is a side effect on typing state from a pass documented pure; look up by the template id components instead, or have `FunctionCallTE.callable` carry the template.

**4e. Flatten `GroupSubtree`'s entry state.** `invalidated_by` lives per (node, key), so `is_use_after_churn` scans the whole tree per use (check_usages.rs:401-405), and `merge` must walk two cloned trees in lockstep with two `expect("branch dropped ...")` (check_usages.rs:411-450). A reference is stale iff *any* of its registrations is, so keep one flat `IndexMap<RefKey, LocalEntry>` and let tree nodes hold `IndexSet<RefKey>` membership only. Merge becomes a single map fold; branch clones shrink to the membership sets.

**4f. Deferred cases should be errors, not panics.** `BorrowErrorKind::UnderivableBorrowGroup` is defined (borrow_error.rs:40) and never constructed anywhere; `groupify_function` returns `Result` and never returns `Err` (groupify.rs:45-55). Meanwhile weak locks, closure captures, nested unannotated borrows, `in x` local groups, and group template args all `panic!` (borrow_types.rs:298, 347, 371, 464, 516; groupify.rs:412, 552). A user writing `lock(weak)` with the checker on crashes the compiler. Route every "deferred case" through `UnderivableBorrowGroup.at(range)`, which is what the variant and the `Result` were added for. Same for `check_return_group`, which only inspects the outer return borrow (check.rs:41-51); `Opt<&T>` slips past it and panics later in `group_anon(None)`.

**4g. Fallbacks to remove or justify.** `local_type` returns a groupless type for an untracked local (groupify.rs:332-337); `subst_group_expr` keeps an unbound rune (1e); `resolve_callee` `None` means "pure" (1d); `rune_name(..).unwrap_or("?")` (aliasing_info.rs:212). The 13 VLOOOOK markers are the right list to work through.

**4h. Only the first error is reported.** `?` on every arm and `joint_facts(...).first()` (check_usages.rs:127). Fine for now, but the `GroupSubtree` design supports collecting; a `Vec<BorrowErrorKind>` return would let a user fix a function in one pass.

**4i. Small things.** `GroupStep` can be `Copy` (all leaves are). `call.range[0]` for the whole-call range (check_usages.rs:136) deserves a named accessor on `FunctionCallTE`. `UseAfterChurn` wording says "array element" but ellipsis references and future `Variant` children also die (borrow_error.rs:82-88); `UndeclaredChurn` alone starts lowercase. `children()` allocates a `Vec` per node per walk. `make_kind_g` matches SSA by `ITypeST::Call` with exactly two args (borrow_types.rs:184-193), a syntax assumption that goes silently groupless if the scout ever gains a dedicated array type node.

## 5. Test coverage to add (as failing tests first)

85 tests exist and `walk_completeness_tests.rs` is a good idea. Missing, one failing test each: stale ref used in `set`/`return`/`let` (1a); move or drop then use (1b); interface call with `mut(g)` (1c); lambda call churning (1d); nested rune binding (1e); anonymous-group aliasing per the doc example (1f); rune/local name collision in scope numbering (2a); owned `Opt<&T in g>` argument next to a `noalias` candidate (2b); weak lock with checker on yields an error not a panic (4f).
