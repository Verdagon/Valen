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

Turns the checker's aliasing facts into LLVM `restrict`; full RFIGA plan in
`~/.claude/plans/please-plan-out-implementing-fancy-hanrahan.md`, design in `borrowing-design.md`'s
`calculate_aliasing_info` / `Noalias` sections. **Landed on `main`:** the per-parameter `noalias`
attribute, end to end — `calculate_aliasing_info` (`experimental/aliasing_info.rs`) computes it, carried
in side maps (`FunctionAliasingInfoT` keyed by signature, then `FunctionAliasingInfoI` keyed by
instantiated `IdI` — never on AST nodes), emitted by C++ `declareFunction` (`function.cpp`) via the
`Package.paramNoaliasByName` side map read by `lookupParamNoalias` (`boundary.cpp`); test
`end_to_end_tests/tests/noalias.rs`. Restrict regions come from the `AccessEventG` log, with a
bare-integer statement (`103;`) recorded as a `Marker` so a test can pin a region by value. **Next —
Phase B:** block-scoped `!alias.scope`/`!noalias` metadata on the `load`/`store`/`call` in a region where
one reference is the sole user of its group, keyed by `LocT`/`LocI`; RFIGA (B1 checker regions, B2
codegen) in the plan file.

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
- **Carry per-function backend facts in side maps keyed by function id, never as fields on the typing/
  instantiated AST or the metal AST.** Mirror `extern_abi`/`struct_layouts` (signature → `IdI` →
  `Package.paramNoaliasByName`); presence in the map is the "analyzed" signal (absent = not analyzed),
  and codegen asserts a present entry's length matches the param count rather than silently tolerating a
  short/empty one.
