# Opaque types and their IDs — report

Companion to `rust-interop-design.md`'s "Opacity" section and its `__ValeOpaque` TODO. This is the
investigation behind them: where the numeric ID on `__ValeOpaque` came from, what reads it, what a
name could do instead, and what comptime forces. It is a report, not a design; the design doc rules.

**TL;DR.** Today every Valen type that crosses into Rust carries a 64-bit hash, and a lambda functor
crosses as nothing *but* that hash. The hash was inherited from Sky, where it was designed for types
that have no Rust declaration, and was never read back by anything in Sky's code. In Valen it is read
by one site, and that site is why the warm-rebuild ordering rule exists. Every type the typing pass
knows about can be a named Rust struct instead, looked up by path with no table. What genuinely needs
a number is a *value* used as a generic argument, because rustc's const generics cannot spell an
arbitrary Valen value. That number must be a pure function of the value (rustc's incremental cache
replays it across builds), which means a content hash with a collision check, or the value's own
canonical bytes as the const parameter.

## 1. What exists today

The stub declares one wrapper:

```rust
pub struct __ValeOpaque<const T: u64>;
```

and every projected Valen struct wraps it as a field:

```rust
pub struct MyCb<F>(__ValeOpaque<8706161416459359414>, PhantomData<(F)>);
pub struct MainLoop__anon<T0>(__ValeOpaque<…>, PhantomData<(T0)>);
```

The number is `typeid()` in `src/typing/rust_interop/typeid.rs`: FNV-1a over a string. Two different
strings are hashed on the two sides of the boundary:

| side | function | string hashed |
|---|---|---|
| stub generator | `generate_stub_source`, `generate_pass2_stub` (`stub_gen.rs`) | the struct's *declared* name (`"MyCb"`, `"MainLoop__anon"`) |
| instantiator | `opaque_ty`, `collect_callback` (`src/instantiating/rust_interop/mod.rs`) | `humanize_id` of the *instantiated* id |

These agree only in the monomorphic case. That asymmetry is what convo-153 hit when it tried to
resolve the anon substruct by hash ("the humanized instantiated id, with concrete args, isn't
reproducible at stub-gen time") and fixed by giving the substruct a *name* (`anon_substruct_rust_name`).

A lambda functor is the one thing that crosses with no named struct at all. `citizen_or_opaque_to_rustc_ty`
lowers it to a bare `__ValeOpaque<typeid(humanize_id(lambda))>` as a type argument:
`MainLoop__anon<__ValeOpaque<8050229659068875523>>`.

### Who reads the number

- **The C++ backend: nobody.** `compute_struct_layouts` keys its layout map by humanized id string.
  `Backend/src/rust_interop/` has no occurrence of `typeid` or `Opaque`.
- **`read_opaque_typeid` + the universe in `collect_callback`.** When rustc asks for a callback's
  `per_instance_mir`, the instance's args carry `__ValeOpaque<N>`. `collect_callback` builds a
  `HashMap<u64, IdI>` by hashing every type in `monouts` and asserts `N` is present. The actual impl
  match is rustc `Ty` equality on forward-projected candidates, so the table is used only for the
  presence assert. This is the read that made warm rebuilds nondeterministic (design S7).
- **Tests.** `typeid_is_deterministic_and_distinct` pins `typeid("MyCb") == 8706161416459359414` as a
  cross-process reproducibility fence. Drive tests assert `stub.contains("__ValeOpaque<")` with the
  number elided. No hand-written fixture declares `__ValeOpaque`.

## 2. Where the number came from

Sky's `design-convo-log.md:18560-18720`. The question was how a Valen-internal type can appear inside
a Rust generic, `Vec<MySkyInternalType>`, when rustc has no `DefId` for it. Three options:

- **A.** Forbid it. Rejected as too restrictive.
- **B.** Mint real `DefId`s *dynamically inside rustc* with extra fork patches. Rejected: "DefId
  allocator, item-loading machinery, definitely 2+ patches."
- **C.** Pre-declare one generic wrapper `SkyOpaqueType<const T: u64>` and let a content hash stand in
  for the name. Chosen.

Everything else in Sky's design was built on C: the typeid→type universe table, `layout_of` keyed by
typeid, drop glue named `__sky_drop_typeid_N`, cross-crate agreement by hashing the same source, and
comptime-produced types identified by hashing their construction recipe.

Two facts about that origin matter now:

1. **B rejected runtime `DefId` synthesis.** Statically emitting a named struct into the stub before
   rustc starts, which is what Valen's pass 2 does, was never the thing rejected. Exported types
   already did exactly that in Sky.
2. **Sky's design gave closures named structs.** Arch §14.1: `__sky_closure_42(SkyOpaqueType<HASH>,
   PhantomData<..>)` with an `Fn` impl, because an impl needs a `DefId` to attach to. Valen's
   bare-opaque functor is a Valen deviation. It entered in convo-148 when the architect corrected
   the model away from *real fields* toward the opaque wrapper; making the functor bare (arch §10.7
   Case 2) rather than named (§14.1) was the model's reading, not a ruling.

**The wrapper-as-field shape** is separate from the number. Sky's first shape was "the opaque *is* the
type" with zero source fields, and rustc's debuginfo walker ICE'd whenever such a type sat inside a
Rust generic, because the walker assumes source field count equals layout-reported field count. The
fix was to make the opaque a field of a named struct. That constraint is "report the same number of
fields you declared"; it does not require any particular field, and it does not require the number.

### What Sky's code actually did with it

Nothing. `lang_layout_of` dispatches by item *name*. The decode helpers (`extract_typeid_from_args`,
`build_opaque_args`) are `#[allow(dead_code)]` with "no in-tree consumers." The typeid table is
serialized into the cache and never read back. `typeid::compute(name, &[])` is called with empty
type args at every site, so `Wrapper<i32>` and `Wrapper<i64>` share one typeid and are distinguished
only by the struct's own generic params. The hash was a speculative layer whose consumer never landed.

## 3. What a name does instead

rustc already gives every struct in the stub a `DefId` and a path. When rustc asks about
`MyCollection<i64>`, we get the path and the args:

- path → the Valen template, from the typed program;
- `i64` → `int`, by inverting the same lowering we run forwards;
- substitute, and answer `layout_of` or instantiate the override.

No table, no ordering. This is what the anon substruct already does (`resolve_local_type` by
`anon_substruct_rust_name`), and it generalizes to every type the typing pass has seen, lambdas
included. Valen names every lambda (`lambda#3` under its enclosing function); what the lambda lacks is
a Rust *declaration*, and the pass-2 generator can emit one.

Under that rule `__ValeOpaque` loses its parameter. Its remaining jobs are the two marker fields:

```rust
pub struct __ValeOpaque(PhantomData<*mut ()>, PhantomPinned);
```

which make every Valen type `!Send + !Sync + !Unpin` unless we explicitly emit an impl (arch §10.6,
HBAB §26.20). Those markers can equally be fields of each generated struct directly; the wrapper is a
way to write them once.

### The three cases the design settled on

| Valen type | Rust side | identity |
|---|---|---|
| exported | named `pub struct`, markers + `PhantomData<generics>` | path |
| implements a Rust trait (exported or not) | named struct, **not `pub`** if not exported | path |
| everything else that appears in a Rust type slot | design TODO: named gensym struct, or bare `__ValeOpaque<OID>` | path, or OID table |

**Implementing a Rust trait forces a name**, because an `impl` attaches to a `DefId`. A struct that is
private to the stub crate satisfies "not exported": rustc knows it, no Rust source elsewhere can name
it, and it reaches Rust code only through monomorphization. Comptime arguments do not change this;
they ride along as `const N` (§4).

### Cost of naming the third case

Every Valen type that appears inside a Rust generic gets one line in the generated stub. The same set
needs an entry in the OID table under the opaque design, so the count is identical; only the form
differs. Fields are never named, so there is no recursive closure to declare.

## 4. What comptime forces

The architecture locks comptime as **value-only** (§13.4): comptime never produces a new type; it
selects or parameterizes declared ones. So every Valen type has a declared template with a name. The
number's job reduces to standing in for a *value* in a generic-argument position:

```
struct MyStruct<config: Config> { ... }      // Valen: a struct-valued generic parameter
MyStruct<Config(42, "hello")>
```

rustc's const generics accept integers, bools and chars, not `Config`. So:

```rust
pub struct MyStruct<const N: u128>(markers…);     // Rust: the value, as a number
impl<const N: u128> Tr for MyStruct<N> { #[vale::emit_consumer_body] fn m(…) }
```

`MyStruct<Config(42, "hello")>` crosses as `MyStruct<7781>`, `per_instance_mir` reads `7781` off the
instance and looks the value up. This is a plain `const`, not `__ValeOpaque`; the type form
(`MyStruct<C>` with `C = __ValeOpaque<N>`) buys nothing here and is needed only when the hidden thing
must sit in a slot rustc insists is a *type*, i.e. a whole opaque type inside `Vec<…>`.

**Values are mostly computed during instantiation.** Under per-Instance evaluation (§13.7) a value
often depends on arguments only rustc supplies (`foo::<i32>` computing `Config(size_of::<T>(), …)`).
Such a value does not exist until the export's `per_instance_mir` runs, so:

- its table entry is written then, and any reader (`layout_of` for `MyStruct<7781>`, a callback on
  it) must run after — this is design S7, exports-first, and it is a standing invariant, not a
  warm-rebuild patch;
- the value cannot be spelled structurally as a Rust const without exposing `Config`'s fields, which
  is the leak the whole design exists to avoid.

## 5. What the number must be

**Not a counter.** Two reasons:

1. `per_instance_mir` is re-fired in an order rustc does not guarantee (that was the P0), so an
   allocation order is not reproducible.
2. **rustc's incremental cache replays our IDs across builds.** On a warm build rustc loads
   `items_of_instance` results from disk — lists of `Instance`s, our generated types and their
   generic args included — and hands us `<MyStruct<7781> as Tr>::m` from *last* build. `7781` must
   mean the same value this build with no help from the run that produced it. A counter assigned in
   a different order would silently dispatch to the wrong value. (The source digest in
   `emit_consumer_body` makes those nodes red when the `.valen` changes, so stale IDs are replayed
   only when the source is unchanged, which is exactly when a pure function of the value is safe.)

So the ID must be a pure function of the value, the same in every build and every crate. Two ways:

**Content hash, 128-bit, with insert-time collision check.** The table stores the full value; the
hash is the key. On insert, same content is fine; different content aborts the build with a clear
message. A collision is therefore loud, never a miscompile. rustc makes exactly this bet on itself:
`DefPathHash` is `Fingerprint(u64, u64)` and `rustc_span/src/def_id.rs:59-65` says the compiler
"actively and exhaustively checks for such hash collisions and aborts compilation if it finds one",
quoting 1 in 14.75 billion for `rustc_middle`'s ~50,000 items on the 64-bit crate-local half. Today's
code uses 64 bits; the architecture doc locks 128.

**The canonical bytes themselves as the const parameter.** Under `adt_const_params` (a nightly
feature; `valenc-rs` is nightly-only and the stub already uses `register_tool`), a const generic may
be `&'static [u8]` or `&'static str`: `MyStruct<const V: &'static [u8]>` with `V` = the value's
canonical serialization. Injective by construction, no table needed for identity, no collision, no
ordering hazard for identity. Costs: one feature attribute, long mangled symbols (v0 mangling embeds
the bytes), and the value's bytes are visible to rustc, which hides *types'* fields, not values.

## 6. Drop glue

A named struct gets its own `Drop` impl like any other projected type. A bare `__ValeOpaque<OID>` does
not: `Vec<__ValeOpaque<N>>` going out of scope drops each element, and a wrapper with no `Drop` impl
makes that a no-op, so the Valen destructor is skipped and a linear type loses its guarantee. If the
bare form is kept, the stub needs one blanket impl:

```rust
impl<const OID: u128> Drop for __ValeOpaque<OID> { #[vale::emit_consumer_body] fn drop(&mut self) { … } }
```

so `per_instance_mir` receives the instance with `OID` in its args and instantiates that type's drop.

## 7. What rustc ever asks about an opaque

Only three things: size and alignment (`layout_of`), auto-traits (answered by the marker fields), and
drop glue. Anything that would *call a method* on a Valen type needs an `impl`, and an `impl` needs a
named type. "Methods force a name" is the boundary between the named and numbered forms.

## 8. Tables and ordering

Every table the instantiator fills is written by an export's `per_instance_mir` and read by something
rustc asks about later:

| table | written by | read by |
|---|---|---|
| types + impls (`monouts`) | export instantiation | `collect_callback` (today, via the hash universe) |
| OID → type (if the bare form is kept) | export instantiation | `layout_of`, `Drop` |
| comptime integer → value | export instantiation | `layout_of`, callbacks on `MyStruct<N>` |

Named-struct identity removes the first row's dependency (a callback names its own override and
instantiates on demand). The other rows remain, and comptime makes the third row the normal case.
Hence design S7: re-fire every export first, from our own sorted export list, then everything else.
Structural bytes (§5) remove the *identity* dependency of the third row but not the need to have
computed the value before anything is lowered against it.

## 9. Open decisions, as the design doc leaves them

- **The `__ValeOpaque` TODO**: keep a bare numbered form for non-exported types in Rust type slots, or
  name everything. The report's finding is that naming costs the same number of entries, removes a
  second identity mechanism, the hash universe, the blanket `Drop`, and one table from S7's list; what
  the numbered form preserves is the preference that rustc not see a declaration per crossing type,
  and a gensym reveals essentially the same nothing.
- **Hash vs canonical bytes** for comptime values (§5).
- **64 vs 128 bits** if hashing: the code says 64, the architecture doc says 128.

## Sources

- Sky: `/Volumes/V/Harmonious/design-convo-log.md` (18560-18720, 19570, 19705; 10611-10974 for the
  original opacity rationale), `convo-with-rustc.md` (Q8/Q9/Q10/Q16), `rust-interop-architecture.md`
  §8.5, §10.6–10.9, §13.3, §14.1, §29.A; `toylangc/src/typeid.rs`, `oracle.rs`,
  `rustc-lang-facade/src/queries/layout.rs`; `docs/historical/rust-interop-architecture-v1.0.md`,
  `course-correct.md`, `docs/historical/struct-opacity-and-type-deps.md`.
- Valen: `docs/architecture/vale-rust-interop-architecture.md` §10, §13, §14, §26.20;
  `docs/convos/` 81–83, 86, 123, 148, 150, 153, 154; `src/typing/rust_interop/typeid.rs`,
  `stub_gen.rs`, `src/instantiating/rust_interop/mod.rs`.
- rustc: `compiler/rustc_span/src/def_id.rs` (the `DefPathHash` collision policy).
