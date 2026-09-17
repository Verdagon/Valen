# Replace the process-global `fill_extra_modules` hook with a field on `LlvmCodegenBackend`

## Context

Patch 2 of the Valen rustc fork (`fill_extra_modules`) lets `valenc-rs` hand rustc extra LLVM modules that ride rustc's own optimize/ThinLTO/emit pipeline. The trait-method half (`ExtraBackendMethods::fill_extra_modules(&self, tcx) -> Vec<ModuleCodegen<Self::Module>>`) mirrors upstream's `codegen_allocator` and is the upstreamable artifact. The install half is not: `valenc-rs` calls `rustc_codegen_llvm::set_fill_extra_modules_hook(consumer_fill_modules)`, which stores the fn pointer in a process-global `static OnceLock`. Eight independent investigations of upstream precedent, history, and the dev-guide agreed that the global is the one piece a rustc reviewer would reject: no driver-installable behavior static exists anywhere in `rustc_codegen_ssa`, `rustc_codegen_llvm`, `rustc_interface`, or `rustc_driver_impl`; even a bare `&'static AtomicBool` (`using_internal_features`) is threaded through `Config`; and Valen's own design doc (@OWTHACBZ) calls the global "just *bad*."

Upstream has the exact pattern we need: `rustc_interface::util::DummyCodegenBackend { pub target_config_override: Option<Box<dyn Fn(..)>> }`, a backend value carrying a driver-supplied closure as a `pub` field, installed by miri through `Config::make_codegen_backend` (`src/tools/miri/src/bin/miri.rs:187-197`). `Config::make_codegen_backend`'s own doc says it exists for "a custom driver where the custom codegen backend has arbitrary data." A *wrapper* backend type would not work (Sky tried): `codegen_crate<B>` is monomorphized with `B = LlvmCodegenBackend`, so the field must live on rustc's own LLVM backend type.

Goal: the hook becomes per-backend-instance state, set by the driver through `Config`, with no global anywhere. No behavior change; the existing hook tests pin it.

## The change

### Fork (`~/rust`, branch `per-instance-mir`, uncommitted on upstream `d940e56841d`)

`compiler/rustc_codegen_llvm/src/lib.rs`, the only fork file that changes:

- Delete `static FILL_EXTRA_MODULES_HOOK` and `pub fn set_fill_extra_modules_hook`.
- Keep `pub type FillExtraModulesHook = for<'tcx> fn(TyCtxt<'tcx>) -> Vec<ModuleCodegen<ModuleLlvm>>;` (a `fn` pointer is `Copy`, so `#[derive(Clone)]` stays trivial and no `Box<dyn Fn>` bounds are needed).
- `pub struct LlvmCodegenBackend(())` becomes a named struct with one `pub` field, the `DummyCodegenBackend` shape:
  ```rust
  #[derive(Clone)]
  pub struct LlvmCodegenBackend {
      /// Extra modules an external driver contributes to codegen; see
      /// `ExtraBackendMethods::fill_extra_modules`. `None` for a stock compile.
      pub extra_modules: Option<FillExtraModulesHook>,
  }
  ```
- `LlvmCodegenBackend::new()` constructs `{ extra_modules: None }` (the `__rustc_codegen_backend` dylib entry and the default path stay vanilla).
- `codegen_crate` passes `self.clone()` to `rustc_codegen_ssa::base::codegen_crate` instead of a fresh `LlvmCodegenBackend(())` literal (the one other construction site).
- `fill_extra_modules` impl reads `self.extra_modules` instead of the static: `match self.extra_modules { Some(hook) => hook(tcx), None => Vec::new() }`.

Also remove the four `// DO NOT SUBMIT` markers, since the investigation answered each:

- `compiler/rustc_codegen_ssa/src/base.rs:724-727`: replace the four-line "consider _not_ asking the backend for this" comment with one line stating the resolved fact: extra modules come from the backend the same way the allocator module does (`codegen_allocator`), so cg_ssa stays backend-agnostic.
- `compiler/rustc_codegen_llvm/src/lib.rs:430` (`// DO NOT SUBMIT made pub` on `ModuleLlvm::new`): delete the marker; add a one-line doc comment that an external driver's `fill_extra_modules` hook mints modules through this constructor so they carry rustc's own context/target-machine settings.
- `compiler/rustc_codegen_llvm/src/lib.rs:444` and `:453` (`// DO NOT SUBMIT, exposed this.` on `llcx_raw_mut` / `llmod_raw`): delete the markers; the existing doc comments already state the ownership rule (never dispose; `ModuleLlvm`'s `Drop` owns disposal), which stays.

Untouched: `traits/backend.rs` (the defaulted trait method), `base.rs`'s `backend.fill_extra_modules(tcx)` call itself, `back/write.rs` plumbing, the accessors' bodies. `rustc_interface` needs nothing: `run_compiler` already prefers `config.make_codegen_backend` and then calls `codegen_backend.init(&sess)`, `target_config`, and `provide` on whatever value it got (`interface.rs:456-466, 507`).

### Valen (`src/typing/rust_interop/drive.rs`, AI-editable)

`DrivenCallbacks::config`:
```rust
fn config(&mut self, config: &mut rustc_interface::Config) {
  config.override_queries = Some(vale_override_queries);
  // rustc's own LLVM backend, carrying our hook as instance state: no process-global.
  config.make_codegen_backend = Some(Box::new(|_opts, _target| {
    Box::new(rustc_codegen_llvm::LlvmCodegenBackend { extra_modules: Some(consumer_fill_modules) })
  }));
}
```
The closure must be `Send`; it captures only a `fn` pointer. `NoopCallbacks` (pure-Rust pass-through in `driver/main.rs`) sets nothing, so a non-Valen crate still gets `get_codegen_backend`'s stock backend: @PRCCBIVRZ holds. The harness has no separate `config()` (`harness.rs` delegates to `run_driven_rustc`), so this is the single install site. Both passes of the two-pass driver construct their own backend value, one per `run_compiler`, which is the semantics the global could not express.

`src/instantiating/rust_interop/mod.rs`: two doc comments name the setter (`consumer_fill_modules`'s header, and the null-`DRIVER_STATE` comment that says "the hook is a process-global `OnceLock`"). Rewrite to "installed as `LlvmCodegenBackend::extra_modules` via `Config::make_codegen_backend` from the driven `config()`." No code change.

`src/typing/test/rust_interop/cases.rs`: the doc comment on `rustc_codegen_fires_our_fill_extra_modules_hook` names `set_fill_extra_modules_hook`; reword the same way. Test name and body unchanged.

### Docs (Valen)

- `docs/handoffs/rust-interop-handoff.md`: two passages say the tree "injects bodies via `set_fill_extra_modules_hook(consumer_fill_modules)`" (the "Design doc vs as-built" callbacks bullet) and "installs the query overrides + `set_fill_extra_modules_hook`" (the `run_driven_rustc` bullet). Rewrite both to the `make_codegen_backend` / `extra_modules` field form, present tense, no history (update-handoff rules).
- `docs/architecture/rust-interop-design.md`:
  - Background "Self-evident from the code": the line saying the hook "is the one channel not on the `Callbacks` contract: a fork-patched process-global set via `set_fill_extra_modules_hook`" and the "six points" line become: the hook rides `Config::make_codegen_backend`, a stock `Callbacks`/`Config` seam; the backend value carries it.
  - Add one Design Proposal (next free S number): "The `fill_extra_modules` hook is per-backend-instance state: `valenc-rs` constructs rustc's `LlvmCodegenBackend` with its hook in the `extra_modules` field and installs it through `Config::make_codegen_backend`, the same seam miri uses. No process-global exists in the fork." One sentence of motivation: upstream keeps driver behavior on `Config`/backend values, never on statics.
  - Human-only Design section: the `config` sketch (line ~90) and the "Patches to rustc" inventory (`FILL_EXTRA_MODULES_HOOK` global, the setter) will be stale, and the deviations bullet "Overrides install via config.override_queries, not a custom backend" stays true (query overrides unchanged; the backend is still rustc's own type, not a custom one). Flag all three to the architect; do not edit.
- `docs/architecture/vale-rust-interop-architecture.md` §4.2 / App. B.4 / C.1 describe the `OnceLock` hook; leave for the deviations list (human-owned).

## RFIGA

Refactor with no behavior change: the red is the compile break at the seam, the green is the existing hook tests. One slice.

1. Move the hook from a global to a backend field.
   * R: no new test. Pinned by `rustc_codegen_fires_our_fill_extra_modules_hook`, `rustc_codegen_emits_vale_bodies_into_borrowed_module`, `rustc_driven_bin_links_and_returns_seven` (`src/typing/test/rust_interop/cases.rs`) and the `pipeline_e2e` driver-check/warm-rebuild tests. Update the one doc comment that names the setter.
   * F: fork change first; rebuild the fork (`./x build` then `./x build --stage 2`, both from `~/rust`; ~5 min incremental); `cargo +rustc-fork clean -p frontend_rust`; `cargo +rustc-fork check --lib --features rust_interop`. Expect exactly one error: unresolved `rustc_codegen_llvm::set_fill_extra_modules_hook` in `drive.rs`. Report "failing for the expected reason, proceeding."
   * I: the `drive.rs` change above and the three comment rewrites. Nothing else.
   * G: `cargo +rustc-fork test --lib --features rust_interop -- rustc_codegen_fires_our_fill_extra_modules_hook rustc_codegen_emits_vale_bodies_into_borrowed_module rustc_driven_bin_links_and_returns_seven` passes.
   * A: build the bins (`--bin valenc-rs --bin valen`), then interop `--lib` (pipeline tests run for real), native nextest, wasi nextest; zero new warnings on standalone and interop builds. Then the doc updates.

## Verification

- `git -C ~/rust grep -n "FILL_EXTRA_MODULES_HOOK\|set_fill_extra_modules_hook\|OnceLock\|DO NOT SUBMIT" -- compiler/rustc_codegen_llvm compiler/rustc_codegen_ssa` returns nothing (only Enzyme's FFI lock elsewhere, untouched).
- `grep -rn "set_fill_extra_modules_hook" src/ docs/handoffs docs/architecture/rust-interop-design.md` returns only the human-only Design section lines flagged above.
- Stage1 sysroot still carries the `rustc_*` rmetas after the rebuild (the `docs/build-compiler.md` TRAP).
- Interop `--lib`: 1253 passed, 0 failed, with `automates_driver_check_reverse_callback`, `release_build_of_driver_check_links`, `warm_rebuild_of_auto_generated_forwarder_runs_seven` listed `ok`. Native and wasi: 1168 passed each.
- Pass-through check: a pure-Rust crate through `valenc-rs` still takes `get_codegen_backend` (no `make_codegen_backend` set in `NoopCallbacks`); confirmed by reading `driver/main.rs`, and exercised by every `pipeline_e2e` build (the `nobiliav` dep crate is pure Rust).

## Out of scope (follow-ups the investigation surfaced, not this change)

- Reviewer-shape nits on the trait method: return `Vec<Self::Module>` and let cg_ssa wrap/name them like the allocator; add a `ModuleKind` variant so the LTO policy for extra modules is explicit.
- The raw-handle surface (`ModuleLlvm::new` pub, `llcx_raw_mut`, `llmod_raw`): a sanctioned `unsafe` API story before any PR.
- Writing up the `-Clinker-plugin-lto` comparison in the architecture doc.
