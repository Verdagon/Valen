# Native e2e virtual-interface tests — handoff

**Goal:** get the native end-to-end virtual-interface tests passing. These are the
`#[ignore = "deferred: interface/upcast/downcast"]` tests in `src/end_to_end_tests/tests/`:
`interfaceimm`/`interfacemut` (virtuals.rs), `upcastif` (ifelse.rs), the four `downcast*` (downcast.rs),
the interface extern tests (externs.rs), and the interface header golden (extern_header_goldens.rs).
They all `assert_compile_and_run(...)`, so they compile → link → run a real binary and need a working
clang/LLVM toolchain — distinct from the TestVM integration tests.

Session mailbox identity: `Valen-exp-1-wipbx-olive`. exp-4 (`Valen-exp-4-wipbx-rowan`) collaborates.

## Where they fail: the C++/LLVM backend, not the Rust pipeline

These are dynamic-dispatch tests (they intend virtual dispatch) and they die *below* the typing +
instantiator passes, in the onion C++/LLVM backend. The Rust side is clean for the mutable-interface +
upcast subset — the instantiated-name humanizer's `INameI::Impl` arm (`fn humanize_name`,
`src/instantiating/instantiated_humanizer.rs`) is implemented, so the `Discriminant(7)` panic is gone
and those tests now reach codegen.

Re-map before planning (the map below predates the impl-bound devirtualization that just landed, so a
few entries may have shifted). Run the ignored e2e interface subset and read each failure site:

```
cargo nextest run --manifest-path Cargo.toml --run-ignored ignored-only --no-fail-fast -E 'test(interfaceimm) + test(interfacemut) + test(upcastif) + test(downcast)' > ./tmp/e2e.txt 2>&1
```

As last observed, the non-share/non-weak failures cluster at two C++ backend gaps, both about
*constructing* an interface value (upstream of dispatch):
- **`StructToInterfaceUpcast` is an unimplemented stub** (`translateExpressionInner` in the C++
  `expression.cpp` — an `assert(false)`). Hits `upcastif`, `interfacemutreturnexport`.
- **Interface fat-pointer construction** asserts the LLVM type matches (`makeInterfaceFatPtrWithoutChecking`
  in the C++ `structs.cpp`). Hits `interfacemut`, `interfacemutparamexport`.

So the next lever is C++ backend codegen for concrete→interface upcast + fat-ptr construction — not
Rust frontend work. Confirm the current sites from `./tmp/e2e.txt` rather than these names.

## Out of scope (architect ruling)
Anything gated on **share / imm** (`interfaceimm*`, the LLVM `pass_manager` failures) or **weak** or
**strings** (the `&@str` Reinterpret cluster). The `downcastBorrow*` pair is gated on the deferred
group-generic-closures plan, not the backend upcast gap.

## One Rust-side gap from the devirt work
An impl-bounded method call whose receiver monomorphizes to an **interface** (interface-implementing-
interface) is a deliberate `unimplemented!("interface-receiver impl-bound dispatch")` in the
`BoundFunctionCall` arm of `translate_ref_expr` (`src/instantiating/instantiator.rs`). No current test
reaches it; implement it (rebuild the upcast + dynamic dispatch) only when one does.

## State
Everything is committed and on `main` (`git rev-parse origin/main`); working tree clean. The
impl-bounded method-call devirtualization (`BoundFunctionCallTE`) is landed. Suite counts:
`cargo nextest run --manifest-path Cargo.toml` (native) and `VALE_TEST_BACKEND=wasi ...` — both green,
202 e2e/backend tests skipped (the ignored set).

## Lessons learned
- To find a concrete impl's edge from its *instantiated* id, you need its *typed* id — matching by the
  impl template's `code_location` is fragile because anonymous-substruct impl templates carry no
  `code_location` (only their interface). Record the typed id (and the impl's own bound args) in a
  `monouts` map when the impl is resolved (in `fn translate_impl_id`), so a later body-phase
  devirtualization has them without reading `monouts.impls` (not populated until the impl drains).
- exp-4 preference, generalized: an impl-bounded method call on a *concrete* receiver must devirtualize
  to a static override call, including anonymous-substruct (lambda) impls — don't special-case the
  lambda path into staying dynamic.
- Get ground truth by dumping the actual typing/instantiation output, not by reasoning from the code:
  a plausible "recompute the bound args from the caller's context" fix was disproven in one test run
  (the caller's context lacks the impl's transitive `where func` satisfiers).
- The editor occasionally drops an in-flight edit; after a "changed on disk" notice, re-read the region
  before trusting or building on its state.
