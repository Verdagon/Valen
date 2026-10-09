# driver-check — NobiliaV's frame-loop callback as a `valen build` project

A minimal `valen build` project in the shape of NobiliaV's real driver: `main` hands a lambda
straight to the imported `MainLoopCallback` trait (the compiler synthesizes the anonymous substruct
that implements it), and a generic Rust *method* caller, `NobiliaWindow::main_loop`, calls back into
it every frame. The callback is `on_tick(&mut self, w: &mut NobiliaWindow, input: &FrameInput)`, so
it crosses the boundary with two imported borrow params, and churns the window through `&mut self`
methods.

The pipeline tests in `../pipeline_e2e.rs` stage a copy of this directory into a temp dir and build
it with the real `valen build --no-borrow-check` (churn enforcement through the `where func` bound is
unbuilt, so the borrow checker is stepped around, as NobiliaV does). The loop requests exit at frame
30, and `main` returns 7.

## The vendored `nobiliav`

`Valen.toml` depends on `nobiliav/` beside it: a dependency-free stand-in for NobiliaV's real crate,
keeping only the public API `driver_check.valen` imports (`NobiliaWindow` / `FrameInput` /
`MainLoopCallback` and the arrow-key functions). Its bodies are deterministic and bounded (a frame
counter and an exit flag, no windowing or wall clock), so a run always terminates.

The dependency path is written `../../nobiliav`, not `nobiliav`, because `valen build` renders it
verbatim into the *generated* `target/valen-build/Cargo.toml`, two levels down, and cargo resolves it
relative to that file.

## Build it by hand

From the repo root:

```sh
cargo +rustc-fork build --manifest-path Cargo.toml --features horizon --bin valenc-rs --bin valen
./target/debug/valen build --no-borrow-check \
  --manifest-path tests/rust_interop/horizon/driver-check/Valen.toml
```

`valen` finds `valenc-rs` beside itself. `--no-borrow-check` reaches `valenc-rs` as
`VALEN_BORROW_CHECK=off` on cargo's environment, so anyone driving cargo by hand can set that
variable directly.
