# Borrowing Design

## Design (human-only)

Out of scope:

 * `rc` groups.
 * Return borrows without groups, like `func get(self &IndexMap<K, V>, key K) &V`. When the user writes that, we should give a compile error.
 * Fields that are borrow references, like `struct Moo<g'> { ship &Ship in g; }` or `struct Moo { ship &Ship; }`. When the user writes that, we should give a compile error.
 * Variables that shadow. If we detect this, panic. We don't do shadowing yet in Valen.

### Context

 * The borrow checker is in src/typing/borrow_checker and src/typing/test/borrow_checker.
 * `function_compiler_core.rs` after `coutputs.add_function` is the only place that can call into borrow_checker code, by calling `check_function`.
 * The only public method from the borrow_checker is `check_function`.

### Borrow Checking Happens After Typing (BCHATZ)

`BorrowRef` looks like this:
```
pub struct BorrowRefT<'s, 't> {
  pub inner: KindT<'s, 't>,
}
```
Note how it *doesn't* have a `group: GroupT`. That's because borrow checking is kept separate from type checking.

The borrow checker reads typing pass output, and consults the original postparsed AHT for any groups/annotations, such as `FunctionS`'s `effects` and `ParameterS`'s `tyype: ITypeST`.

`KindT` never contains anything about groups.

Also, because of this, whenever we need to fill a value into a group generic parameter, we just fill it with a `GroupTemplataT{}`.

### check_function Has Three Phases

`check_function` has three phases.

 * It calls `groupify_function`, which makes an AST that has the "true types" of everything (types with groups).
 * It calls `check_usages`, which tracks what references are valid, and checks uses.
 * It calls `calculate_aliasing_info`, which figures out which loads/stores/calls access which groups.

```rs
pub fn check_function<'s, 'ctx, 't, 'g>(
  &self, // Compiler
  coutputs: &CompilerOutputs<'s, 't>,
  function_s: &'s FunctionS<'s>,
  function_t: &'t FunctionDefinitionT<'s, 't>,
  check_arena: &'g Bump,
) -> Result<&'g FunctionAliasingInfoT<'g>, ICompileErrorT<'s, 't>> {
  let body_g = self.groupify_function(coutputs, function_s, function_t, check_arena);
  self.check_usages(coutputs, &body_g)?;
  Ok(self.calculate_aliasing_info()?)
}
```
**This function must stay pure (all immutable inputs, only error+aliasing outputs).**

`check_function` and all the other things it calls will be methods on `Compiler`.

`check_arena` should be made by the caller in the core compiler. In future versions, we'll empty typing's temporary state arena and reuse it for this. Normal arena rules apply: no Vec in it, no Box in it, none of that, use TFITCX instead.

### groupify_function

`groupify_function` produces the new groupified body, an `IExpressionGE<'s, 't, 'g>`.

```rs
fn groupify_function<'s, 'ctx, 't, 'g>(
  &self,
  coutputs: &CompilerOutputs<'s, 't>,
  function_s: &'s FunctionS<'s>,
  function_t: &'t FunctionDefinitionT<'s, 't>,
  check_arena: &'g Bump,
) -> IExpressionGE<'s, 't, 'g>
```

Every borrow's group is derived during groupify from the expression that produces it. Where a group cannot be derived, the checker reports a compile error.

IExpressionGE is similar to IExpressionTE except it has its groups filled in, explained more below.

### G (Grouped) AST

There will be a G variant of most expressions and types.

There's no G variant of function definitions (FunctionT) and type definitions (StructDefinitionT etc.).

The borrow checker doesn't do any interning, it compares all things by equality deeply. This shouldn't be so bad because:

 * They'll usually bottom out in typing pass's outputs which are interned and those comparisons are cheap.
 * It should all be hot in cache, in the 'g arena.

#### IExpressionGE

IExpressionGE is similar to IExpressionTE except it has its groups filled in:

 * Every expressions' result() returns a `KindGT`.
 * Every BorrowRefGT contains a `group: GroupExprG` of where it's borrowing from.
 * Every FunctionCallGE has a `mut_effects: &'g [&'g MutEffectPath]` of what groups it's mutating.
 * Every WhileGE has a `mut_effects: &'g [&'g MutEffectPath]` of what groups were mutated inside it.
    * If a `WhileGE` contains another `WhileGE`, the outer one also contains all of the inner one's mut_effects.

For example, if we have:
```
func foo<l'>(level &Level in l, tile in l.tiles) {
  while true { // Loc: 0,2,1
    print(tile.mana);
    level.tiles.clear(); // Loc: 0,2,1,2,2
  }
}
```

Then `groupify_function` should return an IExpressionGE that looks like IExpressionTE, except:

 * `level`'s type is BorrowRefGT{inner: StructGT(Level's IdT, []), group: GroupExprG::Rune(l)}
 * `tile`'s type is BorrowRefGT{inner: StructGT(Tile's IdT, []), group: ChildElements(Member(Rune(l), tiles))}
 * the `clear` call has a `mut_effects = [[Local("level"), Member("tiles")]]` ("`mut`ated level.tiles")
 * `while` has a `mut_effects: [MutEffectPath([0,2,1,2,2], [Local("level"), Member("tiles")])]` ("a call at 0,2,1,2,2 `mut`ated level.tiles")

All IExpressionGE variants hold expression structs, just like IExpressionTE. Each GE expression struct must have a field `result: KindGT`.

#### MutEffectPath

```rs
// A specific mutation to a specific group (as opposed to GroupExprG which an expression for expressing the group(s) a ref might point at).
struct MutEffectPath<'g> {
  effecting_node_loc: Loc, // Which expr had this mut effect (e.g. loc of `level.tiles.clear()`)
  steps: &'g [&'g GroupStep<'s>], // What group the effect mutated (e.g. ["level", "tiles"])
}
enum GroupStep<'s> {
  Rune(&'s IRuneS), // a group param, e.g. <g'>, resolved to its id
  ParamAnonymousGroup(&'t IVarNameT<'s, 't>), // A param's group if it doesn't come from a rune or another param. The string is the param name
  Local(&'t IVarNameT<'s, 't>), // A local's implicitly declared group.
  Member { member_name: &'s StrI<'s> }, // `x.items`
  ChildElements, // the `[]` part of `x.items[]` if items is a Box/Vec/RSA
  InlineElements, // the `[]` part of `x.items[]` if items is a SSA.
  Variant { variant_name: &'s StrI<'s> }, // an enum's variant, the `WarpEngine` part of `my_ship.engine_enum.WarpEngine`
  // No `Empty` variant, that just becomes not a MutEffectPath at all.
  // No `Union` variant, that just becomes multiple MutEffectPath.
}
```

#### KindGT and ITemplataG

The typing pass never sees groups. So in a way, the typing pass never sees a thing's _true_ type, because the groups have been erased from the typing pass. However, in the borrow checker, we truly do need to see the true type.

That "true type" is `KindGT` (and `KindTemplataG`).

`KindGT` is shaped exactly like `KindT`, except:
 * Its `BorrowRefGT` also contains a `GroupExprG`.
 * It can use T names (IVarNameT, etc.) and T-flavored IDs or expressions when it knows no groups will be in them.
 * Template args are not stored in IDs.

KindGT looks like this:
```rs
pub enum KindGT<'s, 't, 'g> {
  Struct(StructGT<'s, 't, 'g>),
  Interface(InterfaceGT<'s, 't, 'g>),
  StaticSizedArray(StaticSizedArrayGT<'s, 't, 'g>),
  RuntimeSizedArray(RuntimeSizedArrayGT<'s, 't, 'g>),
  BorrowRef(BorrowRefGT<'s, 't, 'g>),
  OwnRef(OwnRefGT<'s, 't, 'g>),
  ShareRef(ShareRefGT<'s, 't, 'g>),
  WeakRef(WeakRefGT<'s, 't, 'g>),
  // These contain nothing interesting to the borrow checker:
  Void(VoidT),
  Int(IntT),
  Bool(BoolT),
  Str(StrT),
  Float(FloatT),
  USize(USizeT),
  Never(NeverT),
  OverloadSet(&'t OverloadSetT<'s, 't>),
  KindPlaceholder(&'t KindPlaceholderT<'s, 't>),
}
```
BorrowRefGT is the interesting one, because it's the only place that has a `GroupExprG`:
```rs
pub struct BorrowRefGT<'s, 't, 'g> {
  pub group: GroupExprG<'s>,
  pub inner: KindGT<'s, 't, 'g>,
}
```

`ITemplataG` mirrors `ITemplataT` but with groups and group-annotated types. It looks like this:
```rs
pub enum ITemplataG<'s, 't> {
  Kind(KindTemplataG<'s, 't>), // Contains a KindGT
  Group(GroupExprG<'s>),
  // The below ones don't have anything interesting for the borrow checker.
  Integer(i64),
  Boolean(bool),
  String(StrI<'s>),
  Prototype(&'t PrototypeTemplataT<'s, 't>),
  RuntimeSizedArrayTemplate(RuntimeSizedArrayTemplateTemplataT),
  StaticSizedArrayTemplate(StaticSizedArrayTemplateTemplataT),
  Function(&'t FunctionTemplataT<'s, 't>),
  StructDefinition(&'t StructDefinitionTemplataT<'s, 't>),
  InterfaceDefinition(&'t InterfaceDefinitionTemplataT<'s, 't>),
  ImplDefinition(&'t ImplDefinitionTemplataT<'s, 't>),
  ExternFunction(&'t ExternFunctionTemplataT<'s, 't>),
  // These two are only ever created by the typing solver, which doesn't handle group information
  Isa(&'t IsaTemplataT<'s, 't>),
  CoordList(&'t KindListTemplataT<'s, 't>),
  // It's weird that this one doesnt have anything interesting to the borrow checker
  Placeholder(&'t PlaceholderTemplataT<'s, 't>),
}
```

As you can see, it's really only types that contain groups. And types that contain types, that contain groups.

The things from typing pass that never even carry a GroupTemplataT don't even need corresponding KindGT/ITemplataG things.

The G AST doesn't store template args in the name, it stores them next to the old IdT:
```rs
pub struct StructGT<'s, 't, 'g> {
  pub id: IdT<'s, 't>,
  pub template_args: &'g [&'g ITemplataG<'s, 't>],
}
```


#### `make_kind_g` / `make_templata_g`

groupify_function calls these two functions to make the above grouped AST.

We can make a `KindGT`/`ITemplataG` via `make_kind_g`/`make_templata_g`. `make_kind_g` takes in the typing-pass type, and the original postparsed `ITypeST` (because it still has group annotations like the `in g` in `&Ship in g`), and mashes those together (with knowledge of the groups in the local scope) to make the true type `KindGT`. Same with `make_templata_g`.

`make_kind_g` looks like:

```rs
pub fn make_kind_g(
  &self,
  kind: KindT<'s, 't>,
  tyype: &'s ITypeST<'s>,
  param_name: Option<StrI<'s>>,
) -> KindGT<'s, 't, 'g> { ... }
```

`param_name` is the surrounding parameter, if we're in one. Useful for interpreting `ship: &Ship` as `ship: &Ship in anonymous_ship_group`.

`make_templata_g` looks like:

```rs
fn make_templata_g(
  &self,
  templata: ITemplataT<'s, 't>,
  written: Option<&'s ITypeST<'s>>,
  param_name: Option<StrI<'s>>,
) -> ITemplataG<'s, 't> {
```

#### GroupExprG

As it recurses through the function, it tracks what the "actual types" are. Here, they're `KindGT` instead `KindT`. `KindGT` is generally shaped like `KindT` except its BorrowRefT also contains a `GroupExprG`.

`GroupExprG` looks like this:
```rs
// An expression for expressing the group(s) a function might mutate or a ref might point at (as opposed to GroupStep which is a specific mutation to a specific group).
enum GroupExprG<'s, 't, 'g> {
  Rune(&'s IRuneS), // a group param, e.g. <g'>, resolved to its id
  ParamAnonymousGroup(&'t IVarNameT<'s, 't>), // A param's group if it doesn't come from a rune or another param. The StrI is the parameter's name
  Local(&'t IVarNameT<'s, 't>), // A local's implicitly declared group.
  Member { base: &'g GroupExprG<'s, 't, 'g>, member_name: StrI<'s> }, // `x.items`
  ChildElements { base: &'g GroupExprG<'s, 't, 'g> }, // the `[]` part of `x.items[]` if items is a Box/Vec/RSA
  InlineElements { base: &'g GroupExprG<'s, 't, 'g> }, // the `[]` part of `x.items[]` if items is a SSA.
  Variant { base: &'g GroupExprG<'s, 't, 'g>, variant_name: StrI<'s> }, // an enum's variant, the `WarpEngine` part of `my_ship.engine_enum.WarpEngine`
  Union { members: &'g [&'g GroupExprG<'s, 't, 'g>] }, // This ref points at multiple groups, or this function mutates multiple groups
  Ellipsis { base: &'g GroupExprG<'s, 't, 'g> }, // the `...` part of `x...`
}
```

Notes:

 * In the future, we'll have a GroupExprG::Empty, but we don't have one yet. We'll add it much later, when we want to support empty groups in generic arguments and associated types. We shouldn't add it until then because AI keeps using it as a hack to get around requirements.
    * There is no such thing as a groupless borrow. After groupify_function, **every single borrow ref should have a group**. No empty groups.
    * ParamAnonymousGroup is **only** to be used for the surface-most borrow in a function signature. Okay: `x: &Ship` -> `x: &Ship in ParamAnonymousGroup(x)`. Bad: `y: &Opt<&Ship> in a` -> `y: &Opt<&Ship in ParamAnonymousGroup(x)> in a`.
 * A `GroupExprG`'s runes are always in the current function's (the caller's) namespace.
 * `map`'s GroupSubtree is different than `map.size`'s GroupSubtree. However, in the code that detects a mutation to `map`, we'll make sure it doesn't invalidate references to `map.size`, because `map.size` isn't destructible independently from `map`.

#### groupify_function

Putting it all together, groupify_function walks the typed body once and produces the mirrored IExpressionGE, filling in group information:

 * Every expression's result KindGT,
 * Every borrow's GroupExprG,
 * Every FunctionCallGE's mut_effects
 * The mut_effects aggregated onto each WhileGE.

For every expression, it figures out the result type of it. Examples:
 * It figures out the result of a `items[0]` indexing expression, by getting the element type of the `items` array type.
 * It figures out the return value of a FunctionCallGE node, by looking at the callee and doing the substitutions.

### check_usages

```rs
fn check_usages<'s, 'ctx, 't, 'g>(
  &self,
  coutputs: &CompilerOutputs<'s, 't>,
  function_g_body: &'g IExpressionGE<'s, 't, 'g>,
) -> Result<(), ICompileErrorT<'s, 't>>
```

The checking phase uses that, and tracks what variables are live with these structs:

```rs
struct LocalEntry {
  invalidated_by: Option<Loc>;
}

// A subtree for a group as the containing function knows it. This grows over time as the function learns about new groups.
struct GroupSubtree {
  // enum RefKey { Named(IVarNameT<'s, 't>), Held(u32), }
  locals: IndexMap<RefKey, LocalEntry>;

  // The locals pointing at an ellipsis inside a certain group.
  // For example, in this function:
  //     func foo(vec &Vec<Ship>) {
  //       first_ref &Ship in vec... = vec[0];
  //       vec.append(Ship(42));
  //       print(first_ref.hp);
  //     }
  // At the start we'll just have GroupSubtree{[{vec,None}],[],[]}.
  // After `first_ref =` we'll have GroupSubtree{[{vec,None}],[{first_ref,None}],[]}
  // After `vec.append` we'll have GroupSubtree{[{vec,None}],[{first_ref,Some(...)}],[]}
  //
  // There's no such thing as a child of an ellipsis; doing `&x.hp` on a `&Ship in g...` produces a `&i32 in g...`.
  locals_in_ellipsis: IndexMap<RefKey, LocalEntry>;

  name_to_child: IndexMap<GroupStep, GroupSubtree>;
}
```

`check_usages` does three things:

 * Builds out the GroupSubtree tree as it discovers more locals and held registers.
 * Invalidates entries in the tree as it discovers mut effects.
 * Checks that a function's body only churns parameters in ways that its signature declares `mut` effects for.

#### Local/Register Discovery

As it encounters a local or a "held register" that contains a reference, we'll register it into the `GroupSubtree` tree as still live. A held register is one that's waiting to be passed to a function or another expression, for example if we say `foo(bar(x), baz(y))`, `bar(x)` will be in a register while `baz(y)` is evaluating. Treat it like a temporary unnamed local. Then, when the call finally happens, we'll do a final usage check on those references.

As it encounters a usage of a reference, whose `BorrowRefGT` will have a `GroupExprG` which we'll use to query the `GroupSubtree` tree to see if anywhere it's pointing at has been invalidated.

#### Invalidating (Churning)

(To "churn" a thing means to invalidate all references into that thing's descendant groups.)

As `check_usages` encounters `MutateGE`s and `FunctionCallGE`s which have `mut_effects: &'g [&'g MutEffectPath]`, it will flatten each `MutEffectPath` into lookups into that `GroupSubtree`.

If it was a `mut(g)` effect (no ellipsis), then:

 * For every `ChildElements`/`Variant` descendant of that path, and all descendants of those `ChildElements`/`Variant` descendants, fill the `invalidated_by`.
 * For each entry in this group's `locals_in_ellipsis`, fill the `invalidated_by`.
 * For each descendant group, for each of their `locals_in_ellipsis`, fill the `invalidated_by`.
 * For each ancestor group, for each of their `locals_in_ellipsis`, fill the `invalidated_by`.

Either way, any reference to the churned group itself survives. Example: `inv(arr)` moves the buffer, so `&arr[0]` dies but `&arr` is fine.

An example:
```
struct Inventory { items Vec<Item>; }
func first<g'>(inv &Inventory in g) &Item in g.items... { ... }
func restock<g'>(inv &Inventory in g) mut(g) { ... }
func main() {
  inv Inventory = ...;
  it &Item in inv.items... = first(&inv);
  restock(&inv); // Churns `inv`, _doesn't_ invalidate refs to inv, _does_ invalidate refs to `inv...`, `inv.items[]...`, etc.
  print(it.name);
}
```

If they mutate a group `g...`, that's the same as mutating group `g`.

Notes:

 * A churn of a group invalidates any references into that group's independently-destructible descendant groups. And the only independently-destructible child groups in Valen are the ChildElements (`[]`) child group and the Variant child group. Therefore, a churn of a group invalidates any references into that group's descendant ChildElements/Variant groups, and all of those groups descendants.

#### Check Argument Overlaps

`check_usages` also ensures that when we're calling a function, we don't borrow something and move it at the same time.

For example, it should reject this:

```
func main() {
  a Vec<Ship> = ...;
  do_something(^a, &a);
}
```

#### Check Group Aliasing

`check_usages` also ensures that the callsite doesn't supply arguments that make two callee groups alias when the callee doesn't expect it.

For example, it should reject this:

```
// grow treats r and s as disjoint: it mutates r while b holds a borrow into s.
func grow(a &Vec<int>, b &Vec<int>) mut(a) { ... }
// Desugared for clarity: func grow<r', s'>(a &Vec<int> in r, b &Vec<int> in s) mut(r) { ... }

func main() {
  v Vec<int> = ...;
  grow(&v, &v); // Reject because &v sends into both r and s, which violates grow's assumption that "r and s are disjoint"
}
```

But it should allow this, because the callee explicitly lets both arguments point into a single group `g'`:

```
struct Entity { hp int; }
func heal<g'>(a &Entity in g, d &Entity in g) mut(g) { }
func main() {
  e = Entity(5);
  heal(&e, &e);
}
```

#### Overrides

For now, when an override implements an abstract method, its declared `mut(...)` must match exactly.

### calculate_aliasing_info

We need to tell LLVM _when_ things are `noalias` (C's `restrict`). And we do that. However, that's only used on function parameters that don't alias.

We _also_ need to communicate where _inside_ the functions what references are temporarily effectively unique. For example:

```
// a and b are both in group g (so they might alias) and it's being mutated,
// so neither can be `noalias`.
func do_things<g'>(a &Ship in g, b &Ship in g) mut(g) {
  set a.fuel = a.fuel + 1;
  set b.fuel = b.fuel + 1;

  // In this trailing part, only `a` reaches into g, and b is never touched again,
  // and do_unrelated doesn't churn g. So `a` is restrict here, despite sharing g.
  set a.fuel = a.fuel + 1;
  do_unrelated();
  set a.fuel = a.fuel + 1;
}
```

So, `a` is restrict for part of that function. The borrow checker should communicate enough such that LLVM can figure out how it can do these "local restrict"-like optimizations.

To do that, we have `calculate_aliasing_info`.


`calculate_aliasing_info` is a function that figures out which loads/stores/calls access which groups, so that we can inform the backend who informs the optimizer so it can optimize better.

Specifically, the backend will be attaching this kind of metadata:

 * An `!alias.scope {scope number}` for every load and store, saying what group it's accessing.
 * A `!noalias {scope number}` for every load, store, and callsite.
 * A `!noalias` for every parameter that is the only reference pointing into its group (like C `restrict` pointer or Rust `&mut`).
 * `readonly` on each borrow parameter if nothing reachable by it can be mutated.

For example, the backend adds this commented information:

```
struct Level { tiles Vec<Tile>; entities Vec<Entity>; }
struct Tile { ... }
struct Entity { ... }
func attack<l', s'>(
    level &Level in l,
    tile &Tile in l.tiles[],
    foe &Entity in l.entities[] mut,
    something &Something in s,       // `restrict`, `readonly`
) {
  // group l is !alias.scope {0}
  // group l.tiles[] is !alias.scope {1}
  // group l.entities[] is !alias.scope{2}
  // group s is !alias.scope{3}

  // Accesses l (and l.tiles[] or l.entities[]) but not s. Readonly.
  print_level(level); // !noalias {3}

  // Accesses l.tiles[], doesn't access l or l.entities[] or s
  heat = tile.heat; // !alias.scope {1} + !noalias {0} + !noalias {2} + !noalias {3}

  // Accesses l.tiles[], doesn't access l or l.entities[] or s
  describe(tile); // !noalias {0} + !noalias {2} + !noalias {3}
  // (calls dont get !alias.scope)

  // Accesses l.entities[], doesn't access l or l.tiles[] or s
  set foe.hp = foe.hp - heat; // Both get: !alias.scope {2} + !noalias {0} + !noalias {1} + !noalias {3}

  // Accesses s, doesn't access l or l.tiles[] or l.entities[]
  print(something); // !noalias {0} + !noalias {1} + !noalias {2}
}
```

We supply information to the backend to do that, which looks like this:

```rs
fn calculate_aliasing_info<'g>(
  &self,
  function_s: &'s FunctionS<'s>,
  function_t: &'t FunctionDefinitionT<'s, 't>,
  body: &'g IExpressionGE<'s, 't, 'g>,
) -> &'g FunctionAliasingInfoT<'g> { ... }
```

...which is included in the HinputsT, per function:

```rs
pub struct HinputsT<'s, 't> {
  ...

  // If the borrow checker is off, this map will be empty.
  // If the borrow checker is on, every single function will have an entry here.
  pub signature_to_aliasing_info: IndexMap<SignatureT<'s, 't>, &'t FunctionAliasingInfoT<'s, 't>>,
  ...
}
```

FunctionAliasingInfoT looks like this:

```rs
pub struct FunctionAliasingInfoT<'s, 'x> {
  // For each param, whether to add `noalias` to it.
  // True if the parameter is the *only* way to reach into a certain group, so add `noalias`.
  // False if other parameters might overlap it.
  pub param_index_to_noalias: &'x [bool],

  // For each param, whether to add `readonly` to it.
  // True if nothing reachable by this parameter is ever modified by this function.
  // False if either:
  //  * A mut effect modifies it, an ancestor, or a descendant.
  //  * A mut effect modifies anything reachable by this param.
  pub param_index_to_readonly: &'x [bool],

  // The _size_ of this slice is all that really matters.
  // The elements of this slice are only for debugging.
  pub group_paths: &'x [GroupIdT<'s, 'x>],

  // Key: LocT for the instruction (load, store, call).
  // Value: All the group IDs that it accesses.
  // (sorted by key)
  // Every load/store/call gets one of these.
  pub instruction_loc_to_accessed_groups: &'x [(LocT<'x>, &'x [u32])],
  // Backend will add the inverse set; LLVM only cares about "groups this instruction *doesnt* access".
  // Note that this **doesn't** say whether or not it mutates; this doesn't correspond to mut effects.
  // It just corresponds to what things are reachable.
}
// The name of each group, just for debugging purposes.
pub struct GroupIdT<'s, 'x> {
  pub steps: &'x [GroupIdStepT<'s>],
}
// A step in a name of each group, just for debugging purposes.
pub enum GroupIdStepT<'s> {
  Rune(StrI<'s>),
  ParamAnonymousGroup(StrI<'s>),
  Local(StrI<'s>),
  Member(StrI<'s>),
  ChildElements, // the `[]` part of `x.items[]` if items is a Box/Vec/RSA
  InlineElements, // the `[]` part of `x.items[]` if items is a SSA.
  Variant(StrI<'s>), // an enum's variant, the `WarpEngine` part of `my_ship.engine_enum.WarpEngine`
}
```

So the above function would produce a `FunctionAliasingInfoT` with this (commented) information:

```
struct Level { tiles Vec<Tile>; entities Vec<Entity>; }
struct Tile { ... }
struct Entity { ... }

func attack<l', s'>(
    level &Level in l,               // param_index_to_noalias[0] = false, param_index_to_readonly[0] = false
    tile &Tile in l.tiles[],         // param_index_to_noalias[1] = false, param_index_to_readonly[1] = true
    foe &Entity in l.entities[] mut, // param_index_to_noalias[2] = false, param_index_to_readonly[2] = false
    something &Something in s,       // param_index_to_noalias[3] = true,  param_index_to_readonly[3] = true
) {
  // group_paths.len() = 4
  // Contents (not semantically necessary, but produced for debugging):
  //  0. [Rune("l")]
  //  1. [Rune("l"), Member("tiles"), ChildElements]
  //  2. [Rune("l"), Member("entities"), ChildElements]
  //  3. [Rune("s")]

  print_level(level); // accesses group 0 + 1 + 2 (l can reach those, can't reach s)
  heat = tile.heat; // accesses group 1
  describe(tile); // accesses group 1
  set foe.hp = foe.hp - heat; // accesses group 2
  print(something); // accesses group 3
}
```


#### readonly parameter

One might think we put `readonly` on a parameter if we mutate nothing in its _owned_ hierarchy. That's wrong.

We put `readonly` on a parameter if we mutate nothing _reachable_ by it.

In this example:

```
func moo<g'>(
   ships &Vec<Ship> in g mut,
   my_opt &Opt<&Ship in g>
) { ... }
```

`my_opt` does _not_ get `readonly`, because it can reach mutable `Ship`s in `g`.

#### A note on redundancy

We technically don't need to put `!alias.scope` on most of these things. Technically, in the original example, we could get away with just these annotations:

```
struct Level { tiles Vec<Tile>; entities Vec<Entity>; }
struct Tile { ... }
struct Entity { ... }
func attack<l', s'>(
    level &Level in l,
    tile &Tile in l.tiles[], // `readonly`
    foe &Entity in l.entities[] mut,
    something &Something in s,       // `restrict`
) {
  // group l is !alias.scope {0}
  // group l.tiles[] is !alias.scope {1}
  // group l.entities[] is !alias.scope{2}
  // group s is !alias.scope{3}

  print_level(level);

  heat = tile.heat; // !alias.scope {1} + !noalias {2}

  describe(tile); // !noalias {2}

  set foe.hp = foe.hp - heat; // !alias.scope {2} + !noalias {1}

  print(something); // !noalias {1} + !noalias {2}
}
```

Claude's explanation for why we can drop these:

 * All !noalias {0} (group l). Nothing directly loads/stores level's own storage (level is only handed to print_level, a call, which gets no !alias.scope), so scope 0 has no members and excluding it does nothing.
 * All !noalias {3} (group s). Same, something is only passed to print(something), a call, so nothing is !alias.scope {3}; excluding it is inert. (This is also the restrict param, so it's covered by something's whole-function noalias regardless.)
 * print_level's only scope tag was !noalias {3} → gone.

However, we produce this information anyway, for simplicity. We might decide to elide some of these annotations later. For now, we emit them for simplicity and consistentcy.


### Closures

the problem arises when we want these two properties:

 * every function is borrow checked independently of any other function
 * we dont want any lifetimes figured out in the typing pass

This is the case:

```
func foo<g'>(...) {
  x Vec<&Ship in g> = ...;
  y Vec<&Ship in g> = ...;
  take_closure({
    print(x.len() + y.len());
  });
}
```

There are a few questions that arise in this example

question 1: what is the type of the closure's underlying struct? more specifically, what are its generic parameters? two options:

 * option A (wrong): `struct foo_closure_1<x_lifetime, y_lifetime, x_T_lifetime, y_T_lifetime> { ... }`
 * option B (correct): `struct foo_closure_1<x_lifetime, y_lifetime, g> { ... }`

question 2: assuming option B above, who figures that out? in other words, who knows that x's

 * wrong answer: just look at the types the user wrote; they wrote &Ship in g so you can see that they both use G. this is wrong because it breaks if the user doesn't manually specify the types (x = ...)
 * correct answer: the borrow checker that runs on foo figures it out

the trouble here is: when we're borrow checking closure's `__call` method, we need to know what the lifetime parameters are of that struct

in other words, foo's borrow checker makes information needed by `__call`'s borrow check

i think the answer is unfortunately to order them; borrow check the parent first, then borrow check the child 

rustc concluded something similar (order them) though they went the opposite direction: borrow check the child first, then feed constraints up to the parent 


aha, found a weakness in my framing. these seemed to be the two options:

 1. borrow check child, then parent. child looks at what it calls, to establish constraints (such as x_T_lifetime == y_T_lifetime).
    * this is nice because we can calculate the child effects (such as "i the child mutate x.items[]") and those propagate upward to the parent.
 2. borrow check parent, then child.
    * this is nice because at the parent callsites we can figure out the actual lifetime parameters that we want the child to have ("i the parent am handing you Vec<&Ship in g> and Vec<&Ship in g>, both of those are g, so you only need one lifetime parameter for it)

both aren't great:

 * #1 is a workaround because the child cant know the actual generic params, that's why it needs to communicate constraints upward.
 * #2 actually fails because the parent needs to know the child's effects.

but there's a middle ground:

do #1, but pause borrow checking the parent while we immediately go borrow check the child.
once the child is done borrow checking, we will then know its effects, and we can resume borrow checking the parent.


### Notes

 * Every extern gets `nounwind` because we only ever compile Valen with panic=abort.

## Design Proposals

**Experimental's `check_usages` is a backward liveness walk.** It walks the grouped body in reverse evaluation order
carrying the reference values that still have a use later in the program, keyed by `RefKey`, each with
its use's range and every group path its type mentions (outer borrow, nested borrows, citizen group
args). A node that consumes references registers every reference-valued child before walking any
child. A churn (a call's `mut_effects`, or the loop's applied to what is still pending at the body's
start) that reaches a pending value is a use-after-churn, reported at that use. A node that produces a
reference (a lookup, a call, a lend) retires its entry; a node that forwards a value (a local read, an
`Unlet`, a `&&T→&T` decay, a block, an `if`'s arms, a `let`, a cast) re-keys it to its source. `if`
walks each arm from the post-`if` set and unions; `while` walks its body from the post-loop set;
`break` resumes from the innermost post-loop set; `return` from nothing. A churn of `P` reaches a
mention `Q` iff `P` is a proper prefix of `Q` and the rest crosses `ChildElements`; an ellipsis mention
dies iff `P` and `Q` are prefixes of each other. Every consumer of a reference is a use: a call
argument, a `return`, a `set` through it, a value read through it, a constructor argument.

**S14. Symphony's `check_usages` is a forward use-site check: state on the group, birth on the
reference.** It carries only a `GroupSubtree` of *churned* groups (`last_mut_effect: Option<{ loct,
range }>` per group node, keyed by `GroupStep`). A churn records itself by walking its group path and
*creating* the node (`entry`/`or_insert`), stamping `last_mut_effect`; nothing else is registered. At
every use — each consuming node's reference-valued children — the check reads the value's group paths
and their `born_at`, and for each path walks the root down the target path: a use is stale when some
node on the way has `last_mut_effect.loct > born_at` **and** the target is an independent descendant of
that node (a `ChildElements`/`Variant` edge lies anywhere between them). The current node's churn is
tested whether or not the deeper node exists, since the tree holds only churned ancestors. Call
arguments are walked in two passes — all args first (so a sibling's churns are stamped), then each
checked — which covers the held-register case with no per-reference bookkeeping.

**S15. A reference carries its birth on its type: `GroupTemplataG.born_at: LocT`.** Groupify stamps the
`LocT` of the node that produced the value, so a copy or temporary inherits the original birth through
its type. A lookup or lend is born at that node; a parameter or rune-map entry at function entry
(`LocT { path: &[] }`); a call's return at the call; a nested/citizen member at the containing value's
birth. A held/returned reference is born at the call site, which is sound because any earlier churn
that matters either spoils a stale argument (caught at the call) or is re-formed by the callee.

**S16. All expression nodes number in one `LocT` scheme** — the typing pass's threaded child-index path
(root `[]`, a node's children at `loct.add(index)` in evaluation order), so lexicographic path order is
evaluation order and `born_at` compares against a churn `loct`. Call and branch nodes number the same
way as lookups (no separate postparser-LID numbering). Declaration-name lifes keep their LID path,
which never collides because a LID path contains no `0`.

**S17. Symphony's use-after-churn diagnostic points at the argument's source location, name-free.** The
error's range is the argument expression's own range and the message is `Used a borrow after
invalidated.` (with the churn's range as the `Invalidated at` note), rather than naming the source
variable — the caret already lands on it, and this avoids tracking which variable an expression read.

**A use-after-churn names its churn, and every violation is reported.** A `MutEffectPath` carries the
churning call's source range, and a `UseAfterChurn` or `UseAfterChurnTemporary` carries it as
`churned_at`; the diagnostic renders `Invalidated at <pos>:` and the call's source line under the
message. `check_usages` collects every violation, sorts them by source offset, and reports one per
use, naming the first churn in program order that reaches it. A function with one violation returns
it as a `BorrowCheckError`; with several, as `BorrowCheckErrors { errors }`, one `ICompileErrorT`
whose rendering is each inner error in full, in order.

**`GroupExprG` is a set of paths.** A reference's group is `&'g [GroupPath]`, and a `GroupPath` is a
`root` (`Rune`, `ParamAnonymousGroup`, or `Local`), `steps` (`Member`, `ChildElements`,
`InlineElements`, `Variant`), and a `descendants` flag for `g...`. A `MutEffectPath` holds one
`GroupPath`; substitution at a call replaces a path's root; register and churn walk `steps`.

**Every non-lambda function carries a written return type.** `FunctionS.maybe_return_type` is `Some`
for every function but a lambda, whether written by the user, written as `void` by the scout, written
by the macros for constructors, drops and forwarders, or written by the interop synthesizer; the checker
reads a callee's return groups off it and treats `None` on a non-lambda as a compiler bug.

**One rune-to-templata map per frame resolves kinds and groups alike.** A frame's map holds an
`ITemplataG` for every generic parameter in scope: a group parameter's entry is
`Group(GroupExprG)`, next to a kind parameter's `Kind(KindGT)`. A function's own definition registers
its group parameters as themselves, `g → Group(Rune(g))`: `build_rune_map` adds a placeholder per kind
parameter, then `register_group_runes` walks each written parameter type against its typed kind and
registers a `GroupTemplataG` for each rune a borrow names plainly (`&T in g`), nested runes first. A
call site's map for the callee binds each callee rune from the grouped arguments (`match_types`, runes
nested in citizen template args included), so `churn<g'>(a &[]int in g)` called with `&arr` holds
`g → Group(Local(arr))`; a borrow-typed template argument of an instantiation leaves its rune unbound,
and the parameter's anonymous group takes over. A placeholder the map binds is substituted; one it does
not bind is itself. Every rune a written type or `mut(g)` mentions is looked up in the frame being
groupified into, and a miss is a compiler bug.

**S1. Aliasing info is region-free.** LLVM should still treat a reference as effectively restrict wherever
it is the sole one reaching its group across a call — but the checker never computes those spans. It
reports only which group each load/store touches and which groups each call can reach through its
arguments, and LLVM concludes the restrict-ness itself from the resulting `!alias.scope`/`!noalias`.
Tagging every access is free because `!alias.scope` is inert without a matching `!noalias`, so the only
load-bearing content is a call's reach, which the backend complements into each `!noalias`.

**S2. A fixed size array's elements share their parent's scope number.** `element_result` gives every
array's elements their own group, whether the array is stored on the heap or kept inline inside its
parent. Scope numbering must fold the inline case back in: an inline array's elements are the same memory
as the parent, so numbering them apart would tell LLVM that an access to the whole parent and an access
to an element never overlap, and it would reorder them into wrong code. A heap array's elements keep
their own number, and the array's type decides which.

**S3. A group rune's entry carries the type of its referent.** `Group(GroupTemplataG { group, kind,
born_at })`: in a definition, `kind` is the referent type of the parameter written `&T in g`; at a
call, it is the bound argument's referent (`born_at` is the value's birth, per S15). A written
`g.items` or `g[]` resolves its step against that type. A rune two parameters share names one group
with two referents, so `kind` is the binding parameter's type, not a property of the group.

**S4. Override effect-matching is a borrow-check.** Override resolution invokes the borrow checker to
compare an override's declared `mut(...)` against the abstract method it implements, and a mismatch is
a `BorrowErrorKind`; so the borrow checker has two entry points — per-body `check_function` and
override-conformance from the impl seam.

**S5. Group-generic closures.** A closure that captures a reference is generic over the groups its captures
need: for each capture, the closure struct gains a group parameter per free group in that capture's type
(found by walking the type, not its definition) plus a fresh outer group for a by-reference capture,
each bound at `&{...}` construction to the enclosing group. The closure body reads them off `self`'s
type, so a captured reference's use is checked like any group-generic call — no cross-function body
peek. Detailed plan in the group-generic-closures design notes.

**S6. A `where func` bound prototype can declare churn.** A bound such as `where func __call(&F, &Win in r,
&Inp) mut(r)` quantifies its region per call (HRTB-shaped), and the checker enforces the declared churn on
every call through the bound. The motivating case is a generic forwarder whose closure parameter churns
a `&mut` window it is handed (a Rust-trait reverse callback); today that program compiles only with the
checker off.

**S7. A trailing `mut` after a borrow parameter's type declares churn.** `w &Win mut` means the parameter's
group is churned by the body, equivalent to naming the group and listing it in `mut(...)`. It is accepted on
function headers, lambda parameters, and `where func` bound prototypes (S6).

**S8. The access log.** `groupify_function` returns, beside the body, an ordered `Vec<AccessEventG>`:
`Read { base_ref, group, loct }` at each `CopyPrim` and at each `Deref` whose result is a value, `Store`
at each `Mutate`, and `Call { touched, loct }` at each function call with the flat group of every borrow
argument. `base_ref_and_group` composes an access's group from the root reference's group plus the
access chain's `Member` and `ChildElements` steps, truncated after the last elements step, or to the
root's group when there is none. `calculate_aliasing_info` reads the log and the parameter paths
(`param_group_paths`), never the tree.

**S9. `groupify_type` walks a typed kind against its written type.** `WrittenContext { type_s, name }`
carries the written type and the parameter it belongs to. A borrow's group is its written `in g`
resolved through the frame's rune map; an unannotated or `held` borrow of a parameter is
`ParamAnonymousGroup(param)`. A citizen's template args pair with a written `Call`'s args
(`citizen_args_in`); a static array's element with `StaticArray<N, T>`'s second arg; a runtime array's
with `[]T`'s element; an own or weak reference's inner with its written inner; a claim's payload with
the same written type. A borrow written as a rune bound to a borrow takes the bound type.

**S10. A lookup's group is its base's path plus one step.** `RuntimeSizedArrayLookup` appends
`ChildElements`, `MemberLookup` appends `Member`, and `StaticSizedArrayLookup` appends nothing, since an
inline element shares its array's group; each path keeps its `...`. A written `g[]` resolves the same
way against the bound referent: `ChildElements` for a heap array, `InlineElements` for an inline one.

**S11. A local read is a borrow of the local in the local's own group.** Groupify tracks each local's
grouped type from its initializer; `LocalLookup(x)` yields `&<that type> in [Local(x)]`, so a reference
local reads as `&&T` and `Deref` decays it. A member read through a borrow-typed temporary with no
`Deref` (a destructure of a borrow) peels to the borrow that points at the struct.

**S12. The phase signatures.** `check_function` first rejects a groupless written return borrow
(`check_return_group`); `groupify_function` returns the body and the access log; `check_usages` takes
the function's scout signature for its declared `mut(...)`; `calculate_aliasing_info` takes the
parameter paths and the log.

**S13. The grouped AST is `ExpressionGE`, one arena struct per node.** `KindGT` and `ITemplataG` are
`Copy`, with compound payloads as `&'g` references into the check arena; `StructGT` and `InterfaceGT`
hold `&'g [ITemplataG]` template args.

## Details

### Phase entry points (from Three Phases, S12)

`check_function` runs `check_return_group`, then the three phases in order, threading phase 1's grouped
AST into phase 2 and its access log into phase 3. All inputs stay immutable; the only outputs are an
error or the aliasing info, so the entry point stays pure.

Phase 1 (`groupify_function`) builds the grouped AST: it fills each borrow's group, attaches each
call's `mut_effects`, aggregating them onto the enclosing `while` node, and records the access log.

Phase 2 (`check_usages`) walks the grouped AST once, backward, and rejects a use of a reference a churn
invalidated.

Phase 3 (`calculate_aliasing_info`) reads the access log and the parameter paths and produces the
`FunctionAliasingInfoT` telling the backend which memory each load, store, and call touches.

### The Symphony use-site check (from S14–S17)

Symphony walks the grouped body forward, carrying one `GroupSubtree` of churned groups. `note_mut_effect`
walks a churn's group path, creating each node with `entry`/`or_insert`, and stamps `last_mut_effect`
on the churned group. `check_kind_still_valid(use_range, kind)` collects the value's `GroupTemplataG`s;
for each group path, `check_target_group_invalidated_since` descends the root then the path steps,
returning whether the target is an independent descendant seen so far (`child_is_independent ||
<deeper>`), and at each node — whether or not the deeper node exists — reports when `last_mut_effect.loct
> born_at` and the target is independent. The error's range is `use_range` (the argument's own range);
`churned_at` is the churn's range (`range[0]`, innermost).

Known gap: this catches a *plain* reference (killed by a churn on an ancestor across a child edge) but
not the ellipsis case. An ellipsis reference `g...` is also killed by a churn at, above, or **below**
its group; the walk-up alone can't see a churn below, so ellipsis needs a subtree check (or a second
"last churn anywhere in my subtree" stamp) — deferred.

### The backward walk's state (from Experimental's `check_usages` proposal)

The walk carries three things and nothing else:

 * `pending`: `RefKey → { use_range, mentions }`, the values with a later use. `register` adds a
   consumed child, keyed as the local it reads (through any decay) at the local's range, or as a held
   temporary at the call or node; merging into an existing key keeps the earlier use and unions the
   mentions. `retire` drops a key at the node that produces the value. `rekey` moves an entry to the
   child that supplies the value.
 * `break_targets`: a stack of post-loop pending sets, pushed on entering a `while`, read at a `break`.
 * `errors`: every violation found; `check_usages` returns them sorted by source offset, one per use.

A `let` forwards its local's pending entry into the initializer; nothing is registered at a binding.
`Destroy` and the static-array destructure drop their destination locals' entries. The producer gate
and the joint-argument facts run at each call, before its arguments are registered.

### Diagnostics (from check_usages)

A use-after-churn of a *named* reference renders as `BorrowErrorKind::UseAfterChurn`, pointing at the
use: the local's own range, or the statement's for a `return`, whose value passes through a result
temporary whose `Unlet` carries the statement's range. A use-after-churn of a *held* temporary — an
unnamed call result — renders as `BorrowErrorKind::UseAfterChurnTemporary`, pointing at the call (the
first entry of `FunctionCallTE.range`). Both carry `churned_at`, the churning call's range, which
`ICompileErrorT::notes()` exposes and the humanizer renders under the message:

```
At test:0.vale:7:11:
  observe(e);
Used e after invalidated.
Invalidated at test:0.vale:6:3:
  churn(a);
```

For a loop's back edge the note names the churning call inside the body, since `While.mut_effects`
shares the calls' `MutEffectPath`s. A use reached by two churns (one per `if` arm, or the body's churn
once directly and once through the `break` path's copy of the post-loop set) is reported once, with
the churn first in program order. Several violations in one function come back as
`BorrowCheckErrors`, rendered as each inner error's full text in sequence.

### Aliasing output (from calculate_aliasing_info)

`check_function` returns a `FunctionAliasingInfoT`, allocated in the check arena. Before that arena is
dropped, `function_compiler_core.rs` copies it into the typing arena so it lives as long as the typed
outputs. It is computed once on the generic function; group structure does not vary per monomorphization,
so every instantiation carries the same facts.

It carries three things, one per optimization:

 * `param_index_to_noalias`: one bool per parameter. True when that parameter is the only reference into
   its group, so the backend adds `noalias` to it.
 * `param_index_to_readonly`: one bool per parameter. True when nothing reachable through that parameter is
   mutated — no declared `mut` covers its group, an ancestor, a descendant, or any group reached through a
   borrow nested in its type — so the backend adds `readonly` to it.
 * `group_paths` plus `instruction_loc_to_accessed_groups`: one entry per scope number (its length is the
   count; each entry's path is debug-only), and for each load, store, and call (keyed by `LocT`) the
   numbers it accesses. The backend adds `!alias.scope` on loads and stores, and derives every `!noalias`
   by complementing against `group_paths.len()`.

The facts ride in side maps keyed by function id, never on `ParameterT`/`ParameterI` or the metal AST:
`HinputsT.signature_to_aliasing_info` (by `SignatureT`), which instantiation copies keyed by the
instantiated `IdI` for the backend. A map entry's presence is the "analyzed" signal; absent means
generated, extern, or the checker was off, and nothing is marked.

A scope number is a piece of memory, folded from each access's group: an inline fixed size array's
elements fold into their parent's number, while a heap array's keep their own (see the allocation-fold
proposal). The value-access nodes (`Deref`/`CopyPrim`/`Mutate`) carry a `LocT` so the backend can look
each access up; they hold only a `RangeS` otherwise, which is not unique enough because a decayed `Deref`
and its inner `LocalLookup` share one.

Externs also get `nounwind` (Vale aborts on panic), which is what lets the scope metadata work across a
call the optimizer cannot see into.

## Test cases

### Array-element churn (rung 2)

```
let arr = [...];     // arr is its own group
let elem = &arr[i];  // elem: a reference into arr's child group (the elements)
churn(arr);          // churn declares mut(g) on its parameter's group
```

`groupify_function` gives the `churn` call's `mut_effects` a `MutEffectPath { effecting_node_loc:
<the churn call>, steps: [Local("arr")] }` — the leading `Local("arr")` names the root — and `elem`'s
type the group `[Local("arr"), ChildElements]`. Walking backward, `check_usages` registers `elem` at its
later use, meets the churn of `[Local("arr")]`, finds that path a proper prefix of `elem`'s that crosses
`ChildElements`, and rejects the use. A reference to `arr` itself is in `[Local("arr")]`, not below it,
and survives; so does a reference to an inline member.

### Use-after-churn through a returned reference (rung 3)

```
let v = map.get(k);   // get returns a reference into an element of self's group
map.remove(k);        // remove churns map (mut on self's group)
print(v);             // stale — use-after-churn
```

`groupify_function` reads `get`'s declared return group (a reference into `self`'s group's elements),
binds `self`'s group rune to the `map` argument, and gives `v`'s `BorrowRefGT` the group
`[Local("map"), ChildElements]`. Walking backward, `check_usages` registers `v` at `print(v)`, meets the
`remove` call's churn of `[Local("map")]`, and rejects the use.

### Held register: use-after-churn through an unnamed call result

```
arr Vec<int> = ...;
use2(get(&arr), churn(&arr));   // get(&arr) returns a reference into arr, held in a register;
                                // churn(&arr) then churns arr; use2 consumes the stale reference
```

Unlike the returned-reference case above, `get(&arr)`'s result is never bound to a named local — it
lives in a register while the sibling argument `churn(&arr)` evaluates. At the `use2` call,
`check_usages` registers both arguments before walking either, keying `get(&arr)`'s result as a held
temporary at the call; walking `churn(&arr)` then meets its churn of `[Local("arr")]`, which reaches the
held entry's `[Local("arr"), ChildElements]`, and rejects the call as `UseAfterChurnTemporary`. A test
using a *named* local would not exercise this — the register must be an unnamed temporary.

## Background

### Self-evident from the code

 * A local's identity is the interned `IVarNameT` (`names.rs`), which the checker uses as the `RefKey::Named` key for a pending reference in the walk; its per-function uniqueness comes from the embedded `LocalNameT.life` (a unique `path: &[i32]` per declaration), so it is safe under shadowing.
 * The value-access and call nodes (`FunctionCallTE`, `BoundFunctionCallTE`, `WhileTE`, `IfTE`, `MutateTE`, `DerefTE`, `CopyPrimTE`) plus the six lookup nodes (`LocalLookupTE`, `ArgLookupTE`, `MemberLookupTE`, `StaticSizedArrayLookupTE`, `RuntimeSizedArrayLookupTE`, `LetAndLendTE`) and `DestroyTE` (`expressions.rs`) each carry a `loct: LocT<'t>` field on one unified threaded numbering (S16); `ExpressionGE::range()`/`result()` (`ast_g.rs`) dispatch the mirror's range and result.
 * `IVarNameT` (`names.rs`) is interned and `Copy`/`Eq`/`Hash`, so it is a hashable local key needing no pointer.
 * `LocT<'t> { path: &[i32] }` (`ast.rs`) is the `Loc`; the same type fills `MutEffectPath.effecting_node_loc` and the access log's `loct`.
 * `return X` lowers to `Consecutor([LetNormal(tmp, X), <drops>, Return(Unlet(tmp))])` (`expression_compiler.rs`), and a reference-local read is `Deref(LocalLookup(x))`; `Deref` is produced only as `&&T→&T` decay, so a value load is always a `CopyPrim`.
 * `while (c) { body }` lowers to `While(Consecutor([If(c, Void, Block(Break)), body]))` (`loop_post_parser.rs`), so a `break` is the only exit besides `return`.
 * `mentions_of` (`experimental/check_usages.rs`) collects every `GroupPathG` a `KindGT` carries: the outer borrow, nested borrows, and the `Group` template args of citizens.
 * `base_ref_and_group` (`experimental/groupify.rs`) walks a grouped access chain through `Deref`/`CopyPrim`/`MemberLookup`/array lookups to its root `LocalLookup`/`ArgLookup` and reads that reference's group.
 * The pipeline already keys locals by name, not pointer: testvm's `VariableAddressV { call_id, name: IVarNameI }` (`values.rs`) does, and its comment records that the typing pass makes the name unique per function (@VCOORD) while the per-mention-reallocated struct pointer is not a stable key.
 * `StructMemberT.name` (`citizens.rs`) is now a `&'t MemberNameT` (`names.rs`, carrying `imprecise_name` + `life`), while the body node the walk actually reads, `MemberLookupTE.member_name` (`expressions.rs`), is still an `IVarNameT` (a `MemberNameT` wrapped as `IVarNameT::Member`). The two are populated independently (e.g. a closure capture at `expression_compiler.rs`), so a member step read off the body node must project the `MemberNameT` out.

### Documented

 * Groups never live on the value type, because a `KindT`'s structural `Eq`/`Hash` is monomorphization
   identity, so a group on `BorrowRefT` would split `Vec<int> in a` from `Vec<int> in b` into two
   monomorphizations — the path-to-borrowing design, §"The group representation". `KindGT` sidesteps
   this by being borrow-checker-only.

### Undocumented

## Open Questions

 * Should an unannotated borrow nested in a parameter's type (`&Opt<&Ship>`) take the parameter's anonymous group, as `groupify_type` does today, or be the deferred error the Design calls for?
 * Which deferred cases should become `UnderivableBorrowGroup` errors rather than panics (the sites are listed under "where it cuts corners" in the handoff)?
 * `groupify_type` substitutes a bound placeholder by rune name; should the rune map key on the placeholder's `IdT` (owning template plus rune) so a caller's `T` never resolves through a callee's `T`?

## Required Reading

 * design-assistant
