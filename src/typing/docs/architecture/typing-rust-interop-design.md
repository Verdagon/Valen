# Typing Pass Rust Interop

This doc `typing-rust-interop-design.md` fills out the details of how rust interop works in the typing pass.

For things outside the typing pass, see `rust-interop-design.md`. If there's anything inconsistent or conflicting between these two docs, please **raise it to the architect**.

## Design (human-only)

By design, Rust interop is mostly abstracted away from the typing pass; the typing pass doesn't really think about Rust interop that much. 

**Valen compiler can be standalone.** The typing pass (and the rest of the compiler) don't _depend_ on anything in `rustc` to compile Valen code. This is also enforced by all the tests that have the `rust_interop` flag turned off. If Rust interop ever requires something to change in the core compiler, it's likely a sign that there is a corresponding bug that we can trigger with pure Valen.

**Typecheck AHT, not Rust Things**. One way we abstract Rust concerns away is that the typing pass doesn't do typechecking against Rust items directly; the Rust interop code first **generates a corresponding postparsed AHT** for that rust, and then Valen type-checks against that. This is true of functions, structs, interfaces, everything.

**Rust Interop is pure** in the typing pass. Every Rust interop function is given a bunch of readonly references. Even the `&CompilerOutputs` is readonly.

## Design Proposals

S1. A Rust trait imports as a synthesized AHT-level `InterfaceS`, the same way a called Rust function imports as a synthesized `FunctionS`. The interface's abstract methods carry the trait's method signatures. There is no separate rust-trait concept — this synthesized interface is the only representation.

S2. Valen typechecks a struct's `impl` of an imported Rust trait, signature match included, with the existing interface/override machinery unchanged. `rust_interop` only synthesizes the `InterfaceS`; a mismatch is a Valen error, not a deferred rustc error on generated source.

S3. The inbound wrapper for a Rust→Valen callback is built during backend codegen, not by a separate pass. This is needed because Rust is going to try to call us with Rust ABI, and we might not be using that ABI. So we need a wrapper they can call that will do the right conversions.

S4. A lambda handed to an imported Rust trait — `SomeTrait((args) => {…})` — gets a compiler-generated forwarder substruct, so no hand-written struct/impl/override is needed. The typing pass fires the same anonymous-substruct macro it fires for a native interface, so the synthesized `InterfaceS` (S1) needs nothing beyond what a native interface has. This covers abstract methods with `&self`, `&mut self`, `&T`, and `&mut T` parameters.

S5. A lambda in a position whose expected type is an imported Rust trait — `w.main_loop((win, inp) => {…})` where `main_loop` is generic over `C: MainLoopCallback` — gets the same forwarder substruct as S4, with no trait name written at the callsite. The typing pass decides which trait the lambda implements from the expected type.

## Details

## Test cases

## Background

### Self-evident from the code

### Documented

### Undocumented

## Open Questions

## Required Reading

 * design-assistant
